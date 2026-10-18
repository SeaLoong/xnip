//! Scheduling facts and decisions, independent of network and process IO.
use super::delivery::Prepared;
use serde::{Deserialize, Serialize};

pub const STARTUP_WINDOW_SECS: u64 = 120;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Usage {
    Cli,
    Mcp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    pub process_id: u32,
    pub kind: Usage,
    pub generation: u64,
    pub activity_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Window {
    pub generation: u64,
    pub opened_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pending {
    pub prepared: Prepared,
    pub first_seen_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallReason {
    IdleStart,
    Deadline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub schema_version: u32,
    pub phase: String,
    pub generation: u64,
    pub last_activity_at: u64,
    /// CLI activity keeps an idle tail; an MCP process is tracked by its live lease.
    pub last_cli_activity_at: u64,
    pub last_spawn_at: Option<u64>,
    pub worker_pid: Option<u32>,
    pub last_check_at: Option<u64>,
    pub next_check_at: u64,
    pub last_install_attempt_at: Option<u64>,
    pub installed_at: Option<u64>,
    pub installed_version: Option<String>,
    pub install_reason: Option<InstallReason>,
    pub pending: Option<Pending>,
    pub window: Option<Window>,
    pub last_error: Option<String>,
    pub last_delivery: Option<super::delivery::DeliveryTrace>,
    pub retained_backups: Vec<std::path::PathBuf>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            schema_version: 1,
            phase: "new".into(),
            generation: 0,
            last_activity_at: 0,
            last_cli_activity_at: 0,
            last_spawn_at: None,
            worker_pid: None,
            last_check_at: None,
            next_check_at: 0,
            last_install_attempt_at: None,
            installed_at: None,
            installed_version: None,
            install_reason: None,
            pending: None,
            window: None,
            last_error: None,
            last_delivery: None,
            retained_backups: Vec::new(),
        }
    }
}

impl State {
    /// CLI callers retain their idle tail; MCP callers rely on the worker's live lease check.
    /// Starts inside one open window share a generation, including mixed CLI/MCP starts.
    pub fn register_usage(&mut self, kind: Usage, now: u64, idle_secs: u64) -> u64 {
        let window_is_open = self.window.as_ref().is_some_and(|window| {
            now >= window.opened_at && now.saturating_sub(window.opened_at) < STARTUP_WINDOW_SECS
        });
        let cli_is_idle = self.last_cli_activity_at == 0
            || now.saturating_sub(self.last_cli_activity_at) >= idle_secs;
        if !window_is_open && cli_is_idle && now >= self.last_activity_at {
            self.generation = self.generation.saturating_add(1);
            self.window = Some(Window {
                generation: self.generation,
                opened_at: now,
            });
        }
        if kind == Usage::Cli {
            self.last_cli_activity_at = self.last_cli_activity_at.max(now);
        }
        self.last_activity_at = self.last_activity_at.max(now);
        self.generation
    }

    pub fn register(&mut self, now: u64, idle_secs: u64) -> u64 {
        self.register_usage(Usage::Cli, now, idle_secs)
    }

    pub fn install_reason(
        &self,
        live: &[Lease],
        now: u64,
        max_defer_secs: u64,
    ) -> Option<InstallReason> {
        let pending = self.pending.as_ref()?;
        if now >= pending.first_seen_at.saturating_add(max_defer_secs) {
            return Some(InstallReason::Deadline);
        }
        let window = self.window.as_ref()?;
        if now < window.opened_at || now.saturating_sub(window.opened_at) >= STARTUP_WINDOW_SECS {
            return None;
        }
        live.iter()
            .all(|lease| lease.generation >= window.generation)
            .then_some(InstallReason::IdleStart)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn pending(first_seen_at: u64) -> Pending {
        Pending {
            prepared: Prepared {
                version: "0.3.0".to_owned(),
                path: PathBuf::from("candidate"),
                sha256: "0".repeat(64),
            },
            first_seen_at,
        }
    }

    fn state() -> State {
        State {
            generation: 4,
            last_activity_at: 1_000,
            last_cli_activity_at: 1_000,
            pending: Some(pending(100)),
            ..State::default()
        }
    }

    fn lease(kind: Usage, generation: u64) -> Lease {
        Lease {
            process_id: 42,
            kind,
            generation,
            activity_at: 1_000,
        }
    }

    #[test]
    fn idle_threshold_opens_one_generation_and_burst_registrations_share_it() {
        for (elapsed, opens) in [(1_799, false), (1_800, true), (1_801, true)] {
            let mut state = state();
            let generation = state.register(1_000 + elapsed, 1_800);
            assert_eq!(generation, if opens { 5 } else { 4 });
            assert_eq!(state.last_activity_at, 1_000 + elapsed);
            assert_eq!(state.window.is_some(), opens);
            if let Some(window) = &state.window {
                assert_eq!(window.generation, 5);
                assert_eq!(window.opened_at, 1_000 + elapsed);
            }
            for _ in 0..16 {
                assert_eq!(state.register(1_000 + elapsed, 1_800), generation);
            }
        }
        let mut first = State::default();
        assert_eq!(first.register(1_000, 1_800), 1);
        assert_eq!(first.window.unwrap().opened_at, 1_000);
    }

    #[test]
    fn mcp_usage_leaves_no_idle_tail_after_its_lease_has_ended() {
        let mut state = state();
        state.last_cli_activity_at = 0;
        assert_eq!(state.register_usage(Usage::Mcp, 1_001, 1_800), 5);
        assert_eq!(state.last_activity_at, 1_001);
        assert_eq!(state.last_cli_activity_at, 0);
        assert_eq!(
            state.install_reason(&[lease(Usage::Mcp, 4)], 1_001, 86_400),
            None
        );
        // The worker cancels a window blocked by an older live session.
        state.window = None;
        assert_eq!(state.install_reason(&[], 1_002, 86_400), None);
        // Once the old MCP lease is gone, the next caller needs no CLI idle delay.
        assert_eq!(state.register_usage(Usage::Mcp, 1_002, 1_800), 6);
        assert_eq!(state.last_cli_activity_at, 0);
        assert_eq!(
            state.install_reason(&[lease(Usage::Mcp, 6)], 1_002, 86_400),
            Some(InstallReason::IdleStart)
        );
    }

    #[test]
    fn recent_cli_activity_blocks_new_windows_for_both_usage_kinds() {
        for kind in [Usage::Cli, Usage::Mcp] {
            for (elapsed, expected) in [(1_799, 4), (1_800, 5), (1_801, 5)] {
                let mut state = state();
                assert_eq!(
                    state.register_usage(kind, 1_000 + elapsed, 1_800),
                    expected,
                    "{kind:?} at {elapsed}"
                );
                assert_eq!(
                    state.last_cli_activity_at,
                    if kind == Usage::Cli {
                        1_000 + elapsed
                    } else {
                        1_000
                    }
                );
            }
        }
    }

    #[test]
    fn mixed_concurrent_starts_share_one_window_without_extending_it() {
        let mut state = State::default();
        for offset in 0..16 {
            let kind = if offset % 2 == 0 {
                Usage::Mcp
            } else {
                Usage::Cli
            };
            assert_eq!(state.register_usage(kind, 1_000 + offset, 1_800), 1);
        }
        assert_eq!(state.last_cli_activity_at, 1_015);
        assert_eq!(state.window.as_ref().unwrap().opened_at, 1_000);
        assert_eq!(state.register_usage(Usage::Mcp, 1_119, 1_800), 1);
        assert_eq!(state.window.as_ref().unwrap().opened_at, 1_000);
        assert_eq!(state.register_usage(Usage::Mcp, 1_120, 1_800), 1);
        assert_eq!(state.window.as_ref().unwrap().opened_at, 1_000);
        assert_eq!(state.last_cli_activity_at, 1_015);
    }

    #[test]
    fn cli_idle_tail_begins_at_the_final_activity_of_a_long_command() {
        for (now, expected) in [(10_799, 4), (10_800, 5)] {
            let mut state = state();
            // A dead lease reports the end of a CLI command that began much earlier.
            state.last_activity_at = 9_000;
            state.last_cli_activity_at = 9_000;
            assert_eq!(state.register_usage(Usage::Mcp, now, 1_800), expected);
            assert_eq!(state.last_cli_activity_at, 9_000);
        }
    }

    #[test]
    fn backward_clock_does_not_open_an_mcp_window_even_without_cli_history() {
        let mut state = state();
        state.last_cli_activity_at = 0;
        assert_eq!(state.register_usage(Usage::Mcp, 999, 1_800), 4);
        assert!(state.window.is_none());
        assert_eq!(state.last_activity_at, 1_000);
        assert_eq!(state.last_cli_activity_at, 0);
    }

    #[test]
    fn backward_clocks_do_not_erase_activity_or_create_an_idle_handoff() {
        let mut state = state();
        assert_eq!(state.register(999, 1_800), 4);
        assert_eq!(state.last_activity_at, 1_000);
        assert!(state.window.is_none());
        assert_eq!(state.register(2_799, 1_800), 4);
        assert_eq!(state.last_activity_at, 2_799);
    }

    #[test]
    fn elapsed_idle_time_without_a_new_invocation_never_grants_installation() {
        let state = state();
        for now in [1_000, 2_800, 5_000] {
            assert_eq!(state.install_reason(&[], now, 86_400), None);
        }
    }

    #[test]
    fn startup_handoff_has_an_exact_bounded_window_and_requires_pending_bytes() {
        let mut state = state();
        state.window = Some(Window {
            generation: 5,
            opened_at: 3_000,
        });
        for (now, expected) in [
            (2_999, None),
            (3_000, Some(InstallReason::IdleStart)),
            (3_119, Some(InstallReason::IdleStart)),
            (3_120, None),
            (3_121, None),
        ] {
            assert_eq!(state.install_reason(&[], now, 86_400), expected);
        }
        state.pending = None;
        for now in [3_000, 86_500, u64::MAX] {
            assert_eq!(state.install_reason(&[], now, 86_400), None);
        }
    }

    #[test]
    fn every_live_old_session_blocks_handoff_while_new_sessions_can_use_old_image() {
        let mut state = state();
        assert_eq!(state.register(3_000, 1_800), 5);
        for kind in [Usage::Cli, Usage::Mcp] {
            let new = lease(kind, 5);
            assert_eq!(
                state.install_reason(std::slice::from_ref(&new), 3_001, 86_400),
                Some(InstallReason::IdleStart)
            );
            for old_kind in [Usage::Cli, Usage::Mcp] {
                assert_eq!(
                    state.install_reason(&[new.clone(), lease(old_kind, 4)], 3_001, 86_400),
                    None,
                    "{kind:?} must not hide a live older {old_kind:?} session"
                );
            }
        }
        assert_eq!(
            state.install_reason(&[lease(Usage::Mcp, 5), lease(Usage::Cli, 5)], 3_001, 86_400),
            Some(InstallReason::IdleStart)
        );
    }

    #[test]
    fn maximum_deferral_deadline_ignores_activity_and_expired_or_absent_windows() {
        for with_window in [false, true] {
            let mut state = state();
            if with_window {
                state.window = Some(Window {
                    generation: 5,
                    opened_at: 3_000,
                });
            }
            let active = [lease(Usage::Mcp, 1), lease(Usage::Cli, 4)];
            assert_eq!(state.install_reason(&active, 86_499, 86_400), None);
            assert_eq!(
                state.install_reason(&active, 86_500, 86_400),
                Some(InstallReason::Deadline)
            );
            assert_eq!(
                state.install_reason(&active, 86_501, 86_400),
                Some(InstallReason::Deadline)
            );
        }
    }

    #[test]
    fn deadline_math_and_generation_are_saturating_at_timestamp_limits() {
        let mut state = state();
        state.generation = u64::MAX;
        assert_eq!(state.register(3_000, 1_800), u64::MAX);
        state.window = None;
        state.pending = Some(pending(u64::MAX - 5));
        assert_eq!(state.install_reason(&[], u64::MAX - 1, 10), None);
        assert_eq!(
            state.install_reason(&[], u64::MAX, 10),
            Some(InstallReason::Deadline)
        );
    }

    #[test]
    fn persisted_state_retains_pending_age_and_handoff_facts_across_restarts() {
        let mut original = state();
        original.register(3_000, 1_800);
        let encoded = serde_json::to_vec(&original).unwrap();
        let restored: State = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(restored.generation, 5);
        assert_eq!(restored.last_activity_at, 3_000);
        assert_eq!(restored.last_cli_activity_at, 3_000);
        assert_eq!(restored.pending.as_ref().unwrap().first_seen_at, 100);
        assert_eq!(
            restored.install_reason(&[lease(Usage::Mcp, 5)], 3_001, 86_400),
            Some(InstallReason::IdleStart)
        );
        assert_eq!(
            restored.install_reason(&[lease(Usage::Mcp, 4)], 86_500, 86_400),
            Some(InstallReason::Deadline)
        );
        let defaults: State = serde_json::from_str("{}").unwrap();
        assert_eq!(defaults.schema_version, 1);
        assert!(defaults.pending.is_none());
    }
}
