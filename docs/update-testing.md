# Silent updater contracts

Update tests compare behavior across execution modes and concurrency scenarios.
Updater process fixtures run against debug executables because production builds deliberately have no endpoint override. Ordinary CLI, MCP and benchmark fixtures set `XNIP_AUTO_UPDATE=0`; updater
contracts use a private state directory and offline release fixtures.

## Observable contracts

| Scenario | Independent oracle |
| --- | --- |
| Explicit opt out, CLI | Exact numbered peek output, successful exit, empty stderr, absent state directory |
| Explicit opt out, live MCP | Initialize and tools/list respond before stdin EOF; every stdout line is JSON-RPC; empty stderr; absent state directory |
| State root is an existing regular file, CLI | Exit status and both streams equal an ordinary invocation; original file bytes remain unchanged |
| State root is an existing regular file, live MCP | Initialize and tools/list still respond; no update text enters either stream; original file bytes remain unchanged |
| Slow or failed release check | Foreground request completes while the offline fixture is blocked; worker records failure after foreground exit |
| Concurrent foreground starts | One process holds the worker lock and one release check occurs |
| Worker termination | OS releases its lock; a later invocation can perform the next check |
| Active MCP session | Pending release remains staged until an idle transition or the maximum deferral deadline |
| Continuous CLI usage | Fresh activity prevents ordinary installation; the maximum deferral deadline still makes progress |
| Next invocation after idle | Staged release becomes eligible once; the new caller's own lease cannot prevent that idle handoff |
| Corrupt download, metadata or checksum | Existing executable bytes remain unchanged; failure is retained in private state |
| Installation and retry | Only the executable at the installation path changes; existing agent connections continue; failures remain silent and retryable |

`tests/update_contract.rs` runs ten real-process contracts on Windows and eight
on Unix. The two additional Windows contracts exercise installation scheduling:

| Test | Executed scenario |
| --- | --- |
| `explicit_opt_out_does_not_create_update_state_or_change_cli_output` | Ordinary peek with updates disabled |
| `explicit_opt_out_keeps_a_live_mcp_session_free_of_update_state_and_messages` | Live MCP with updates disabled |
| `unavailable_update_storage_cannot_change_cli_exit_status_or_streams` | Unusable state directory with a real peek |
| `unavailable_update_storage_keeps_mcp_protocol_and_stderr_unchanged` | Unusable state directory with live MCP |
| `a_blocked_background_check_is_singleton_and_survives_all_foreground_exits` | Eight concurrent CLI processes finish while one metadata HTTP response is blocked; the worker then persists an HTTP failure |
| `live_mcp_remains_usable_during_a_blocked_check_and_after_its_failure` | Two live MCP round trips straddle an HTTP failure; stderr stays empty |
| `a_second_worker_exits_silently_and_process_termination_releases_the_os_lock` | A second worker exits with empty streams; the OS lock is unavailable while the first worker lives and available immediately after it is killed |
| `discovery_commands_do_not_launch_workers_or_create_state` | Version, help and doctor stay local and leave state absent |
| `pending_update_waits_for_mcp_to_close_and_installs_only_on_the_next_usage` (Windows) | A pending image stays staged during live MCP, stays staged after MCP closes, then installs on the next peek invocation |
| `maximum_deferral_installs_during_a_live_mcp_session_and_hands_off_its_monitor` (Windows) | An overdue image installs while old MCP still responds; a worker from the new image keeps monitoring and removes the retained old image after MCP closes |

The concurrency test waits for the loopback server to receive a request before
requiring foreground completion. That proves the worker is still alive while
each CLI has exited, including when stdout and stderr are captured by a harness.
The worker writes the failure marker only after the test releases the blocked
response.

`src/update/policy.rs` has thirteen deterministic timestamp contracts:

| Test | Decision oracle |
| --- | --- |
| `idle_threshold_opens_one_generation_and_burst_registrations_share_it` | Quiet gap immediately below, at and above 30 minutes; 16 burst registrations share the resulting generation |
| `mcp_usage_leaves_no_idle_tail_after_its_lease_has_ended` | A new MCP invocation can open a window immediately after the previous leases end; MCP activity leaves no CLI timestamp |
| `recent_cli_activity_blocks_new_windows_for_both_usage_kinds` | Both CLI and MCP starts respect the CLI idle threshold immediately below, at and above 30 minutes |
| `mixed_concurrent_starts_share_one_window_without_extending_it` | CLI/MCP starts share the startup generation; later starts cannot extend its deadline |
| `cli_idle_tail_begins_at_the_final_activity_of_a_long_command` | End-of-command activity starts the CLI idle interval |
| `backward_clock_does_not_open_an_mcp_window_even_without_cli_history` | An MCP invocation cannot manufacture a handoff after the clock moves backward |
| `backward_clocks_do_not_erase_activity_or_create_an_idle_handoff` | A backward clock keeps the previous activity timestamp and creates no window |
| `elapsed_idle_time_without_a_new_invocation_never_grants_installation` | Time passing alone cannot grant ordinary installation |
| `startup_handoff_has_an_exact_bounded_window_and_requires_pending_bytes` | Before, at and after the 120 second handoff window; absent pending download stays ineligible |
| `every_live_old_session_blocks_handoff_while_new_sessions_can_use_old_image` | CLI/MCP combinations; any older generation blocks installation |
| `maximum_deferral_deadline_ignores_activity_and_expired_or_absent_windows` | Immediately below, at and above 24 hours, including active older sessions |
| `deadline_math_and_generation_are_saturating_at_timestamp_limits` | Timestamp and generation arithmetic do not wrap |
| `persisted_state_retains_pending_age_and_handoff_facts_across_restarts` | A JSON round trip preserves the decision and age of a pending download |

The initial invocation and the first invocation after idle open a short handoff
window. CLI activity retains a 30 minute idle interval measured from the final
activity of a command. MCP usage is tracked by live process leases and leaves
no additional idle interval once those processes close. Both kinds of callers
respect recent CLI activity.

New sessions in one window share a generation and may continue using the image
they already loaded while the worker replaces the installation path. An older
live session cancels the handoff; its later exit alone cannot revive that old
window. A subsequent invocation can open another window when the usage rules
allow it. The maximum deferral deadline remains independent of these windows.

Delivery fixtures in `src/update/delivery.rs` cover checksum-verified ZIP and
tar.gz extraction, metadata and asset validation, older/equal version handling,
bounded HTTP bodies, archive traversal, links, duplicate/missing executables,
installation hash revalidation, preservation of the existing image on failure,
and platform-specific executable replacement. Install candidates use a future fixture version independently of the crate's release version; older and equal versions are rejected separately with valid staged bytes. A local forwarding proxy additionally exercises proxy success, HTTP failure, invalid metadata, an off-channel asset URL, a corrupted checksum and a corrupted archive. Every failed proxy attempt must succeed through a complete fresh direct delivery with one verified staged executable, and retain its route/failure marker. Proxy configuration tests reject HTTP, credentials, paths, queries and fragments. A blocked body fixture verifies that each request uses the remaining shared delivery budget.


## Coordinator filesystem contracts

`src/update/contract_tests.rs` uses real OS locks to verify nonblocking registration during state-gate contention, publication of already-locked leases, completion timestamps, deadline-aware polling, candidate/backup namespace restrictions, obsolete pending release cleanup, and silent recovery from invalid or oversized state. A Windows contract keeps the old state file open across replacement and verifies that the old reader keeps its original bytes while the path exposes the complete new JSON; state writes must not wait or retry.

## Process isolation

Updater tests must not replace `target/debug/xnip` or the user's installed
binary. Installation fixtures copy the built executable into a fresh temporary
directory and start that copy. Release metadata, checksum files and archives
come from a loopback HTTP listener. Production requests keep the official logical channel and use HTTPS transport with proxy-first/direct-fallback routing. Offline process fixtures bypass that proxy and never reach an external release endpoint.

The Windows scheduler installation fixtures append a harmless overlay to the
copied executable so its bytes and SHA256 change while it remains runnable.
They wait for a metadata request to block, hold the real state lock, and inject
a checksum-correct pending image into that isolated installation's state. This
lets the tests cross the scheduling deadline without waiting 24 hours. Download
and extraction validation are covered separately by delivery contracts.

A test that starts a background worker owns its state directory and must wait
for that worker to finish or terminate it before removing the fixture. Child
stdin, stdout and stderr are explicitly disconnected from the worker; a parent
CLI exiting must not keep output pipes open through the worker.

Use operating system file locks for cross-process assertions. An in-memory
mutex or a marker file's existence cannot establish singleton behavior or
crash recovery. A held lease proves that a process is alive; an abandoned lease
file alone cannot keep an update deferred.

## Running

```text
cargo test --test update_contract --locked
cargo test --lib update:: --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

The deterministic scheduler tests should use explicit timestamps, with cases
immediately before, exactly at, and immediately after each deadline. Process
contracts use bounded waits only for observable events such as an HTTP request,
a state marker or a JSON-RPC response. They must not wait for the production
idle window or maximum deferral duration.

Windows replacement and console detachment assertions need Windows. Unix
executable replacement and open-inode behavior need Linux or macOS. Passing one
platform does not establish the other platform's behavior.
