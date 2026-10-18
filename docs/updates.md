# Silent background updates

Automatic updates are enabled for ordinary editing commands and MCP sessions. Version/help/doctor and invalid commands remain local. The foreground only records local usage and starts a detached worker; it never performs a release request or waits for another updater.

The worker has no inherited agent output pipes. All outcomes stay in private state: no success messages, error messages, prompts, notifications, progress bars or update text in MCP responses.

## When installation happens

| Situation | Behavior |
|---|---|
| New release, existing MCP connection | Download and verify in the background; defer ordinary installation |
| CLI invoked repeatedly | Keep a 30-minute tail from the last command's completion |
| All old MCP connections closed, no recent CLI usage | The next invocation opens an installation opportunity |
| New invocation after CLI has been idle for 30 minutes | The next invocation opens an installation opportunity |
| Several agents start together | They share the same 120-second startup window and generation |
| Startup window expires before a candidate is ready | Keep the candidate for a later opportunity |
| A prior session is still alive when a startup window is checked | Cancel that window; ending the session does not itself install |
| Verified update has waited 24 hours | Active usage stops deferring installation attempts |
| No further xnip invocation and no live MCP | Keep pending state for the next use; no permanent idle daemon is required |

MCP usage is tracked for the process/connection lifetime. CLI invocations do not expose their enclosing agent's session lifecycle, so the 30-minute window is a heuristic for CLI-based agents. A command's completion time, rather than its start time, extends that window. Abandoned lease files do not count as live processes.

The startup opportunity is opened by the next foreground invocation, before that invocation's new usage is considered old activity. Installation runs in the background and does not hold up that command. The caller may already be running the previous image. The updater replaces the installation path for subsequent launches; it never terminates or restarts an existing agent connection.

The 24-hour deadline is measured from the first verified prepared release. Discovering another newer release does not reset that deadline. OS permission or storage failures remain silent and retryable; a deadline cannot make an unwritable installation writable.

## Checks, singleton and monitoring

Release checks are due every six hours. Failures use a five-minute retry interval. A live MCP keeps one worker monitoring; after installation, that worker hands monitoring to the newly installed image. Workers exit when monitoring is unnecessary or pending work has become idle and is waiting for the next use.

An OS file lock serializes the entire check/download/install workflow for one installation across CLI and MCP processes, repositories and agent harnesses. Permanent lock files are not deleted. Process termination releases the lock automatically. Startup handoff workers can briefly wait for the previous lock owner to exit; they perform no release work until they own that same lock.

Each usage has its own locked lease, published only after it is locked. A short state gate coordinates registration, decisions and installation. Foreground registration always uses a nonblocking lock attempt. Contention conservatively defers ordinary installation and still ensures a new MCP receives a monitor.

On Windows the worker is created without inheriting handles, with no window and a new process group. On Unix it has its own process group and disconnected standard streams. A blocked network request cannot keep the foreground's stdout/stderr pipes open.

## Release delivery

Only stable versions newer than the running installation are eligible. The logical channel is the official [GitHub latest release endpoint](https://docs.github.com/en/rest/releases/releases#get-the-latest-release):

```text
https://api.github.com/repos/SeaLoong/xnip/releases/latest
```

Transport tries `https://gh-proxy.org/<official-URL>` first, for both metadata and release assets. [GH-Proxy documents this prefix format for API and Releases](https://gh-proxy.com/docs/github-accelerator). An HTTP, JSON, metadata, checksum, archive or extraction failure retries the complete delivery directly against GitHub. Metadata and assets for one attempt stay on the same route; a failed proxy attempt cannot supply a checksum to the direct attempt. A valid cached response saying the current version is latest is accepted until the next scheduled check.

Set `XNIP_UPDATE_PROXY=direct` (or `0`) to use GitHub directly. A custom value must be an HTTPS origin, such as `https://mirror.example`, without a path, query, fragment or credentials. Only public release resources are requested; no GitHub token, project content or agent conversation is sent.

The default proxy is an additional trusted distributor: SHA256 detects corruption, but a checksum and metadata received from the same mirror do not independently authenticate that mirror. xnip currently has no publisher signature verification. Use direct mode when the official GitHub HTTPS channel must be the sole distributor.

Metadata, checksum, archive and expanded executable sizes are bounded. Both routes share a 150-second HTTP delivery budget; each request, including reading its body, is capped at 45 seconds or the remaining budget. HTTPS is enforced on every production redirect, while GitHub's HTTPS CDN redirects remain usable.

The archive SHA256 sidecar is verified; an available GitHub asset digest must also agree. Extraction accepts a regular executable for the selected platform, rejects unsafe paths, links and duplicates, and never executes release install scripts. The extracted executable is hashed again immediately before installation. GNU Linux source builds select the published musl binary for the same CPU.

Unix replacement is an atomic rename on the destination filesystem. Windows stages the complete replacement beside the installation, renames the old image aside and installs the new image, rolling back if the second rename fails. Windows requires a short two-rename pathname gap; existing mapped images keep running. Old mapped images are retained and cleaned by a later worker after Windows permits deletion. Superseded candidates are removed only from the updater's own namespace.

## Internal state and configuration

The default state root is:

- Windows: `%LOCALAPPDATA%/xnip/update/<installation-id>/`.
- Unix: `${XDG_STATE_HOME:-$HOME/.local/state}/xnip/update/<installation-id>/`.

The installation ID derives from the canonical executable path. `state.json` records the last check, next check, phase, worker PID, error marker, pending candidate, first-seen time, installation time/reason and retained backups. `last_delivery.route` identifies the last attempted route (`proxy` or `direct`); `last_delivery.proxy_error` retains a bounded proxy failure even when direct fallback succeeds. It is private scheduling data, not an agent prompt or user-facing log. Lock files, temporary candidates and usage leases live beside it.

| Environment variable | Default | Purpose |
|---|---:|---|
| `XNIP_AUTO_UPDATE` | enabled | Set `0` for pinned deployments, managed installations or offline use |
| `XNIP_UPDATE_STATE_DIR` | platform root | Override the base root; the installation namespace is still appended |
| `XNIP_UPDATE_PROXY` | `https://gh-proxy.org` | Proxy origin first, then direct fallback; `direct` or `0` disables the proxy |
| `XNIP_UPDATE_CHECK_INTERVAL_SECS` | 21600 | Successful check interval |
| `XNIP_UPDATE_IDLE_SECS` | 1800 | CLI idle tail |
| `XNIP_UPDATE_MAX_DEFER_SECS` | 86400 | Maximum usage-based deferral after a candidate is prepared |
| `XNIP_UPDATE_POLL_SECS` | 30 | Background lease/pending poll interval |

Timing overrides accept positive integer seconds. Explicitly disable updates when a deployment must remain pinned; choosing an installer tag alone does not disable future automatic updates. Release binaries use the official channel through the configured transport. Debug builds additionally accept a loopback-only `XNIP_UPDATE_RELEASE_URL` for isolated tests, which bypasses proxy routing; it is not a production channel override.

Tests and benchmarks explicitly set `XNIP_AUTO_UPDATE=0`, except updater contracts that use copied executables, private state and offline HTTP fixtures. See [updater contracts](update-testing.md).
