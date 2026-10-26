# Rust Tunnel Core

<cite>
**Referenced Files in This Document**
- [crates/tunnel-core/Cargo.toml](file://crates/tunnel-core/Cargo.toml)
- [crates/tunnel-core/src/lib.rs](file://crates/tunnel-core/src/lib.rs)
- [crates/tunnel-core/src/contract.rs](file://crates/tunnel-core/src/contract.rs)
- [crates/tunnel-core/src/error.rs](file://crates/tunnel-core/src/error.rs)
- [crates/tunnel-core/src/state_machine.rs](file://crates/tunnel-core/src/state_machine.rs)
- [crates/tunnel-core/src/validate.rs](file://crates/tunnel-core/src/validate.rs)
- [crates/tunnel-core/src/argv.rs](file://crates/tunnel-core/src/argv.rs)
- [crates/tunnel-core/src/ports.rs](file://crates/tunnel-core/src/ports.rs)
- [crates/tunnel-core/src/host.rs](file://crates/tunnel-core/src/host.rs)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs)
- [crates/tunnel-core/tests/fixtures.rs](file://crates/tunnel-core/tests/fixtures.rs)
- [fixtures/argv/control.json](file://fixtures/argv/control.json)
- [fixtures/state-machine.json](file://fixtures/state-machine.json)
</cite>

## Table of Contents

1. [Role and Constraints](#role-and-constraints)
2. [Module Map](#module-map)
3. [Wire Contract](#wire-contract)
4. [Errors](#errors)
5. [State Machine](#state-machine)
6. [Validation](#validation)
7. [Argv Builder](#argv-builder)
8. [Ports](#ports)
9. [Host Implementations](#host-implementations)
10. [Manager](#manager)
11. [Tests](#tests)
12. [Known Gaps](#known-gaps)

## Role and Constraints

`crates/tunnel-core` is the portable Rust implementation of the tunnel domain.
Its crate header states the constraint precisely: it "never names a WebView,
tokio, or HTTP server"; process supervision, TCP probing, and persistence arrive
as injected traits from the composition root. Every module starts with
`#![forbid(unsafe_code)]`.

Dependencies are intentionally small: `serde`, `serde_json`, `thiserror`,
`chrono` (clock + std, no default features beyond that), and `libc`. Rust
edition 2021, `rust-version = "1.77"`, `publish = false`.

The root Cargo workspace builds only this crate by default, so `cargo test` and
`cargo clippy` need no WebView toolchain.

**Section sources**

- [crates/tunnel-core/Cargo.toml](file://crates/tunnel-core/Cargo.toml#L1-L27)
- [Cargo.toml](file://Cargo.toml#L1-L22)
- [crates/tunnel-core/src/lib.rs](file://crates/tunnel-core/src/lib.rs#L1-L18)

## Module Map

| Module | Responsibility |
|---|---|
| `contract` | profile/state/host structs and enums with serde camelCase |
| `error` | 14 closed problem codes, HTTP status mapping, `TunnelError` |
| `state_machine` | allowed transitions, start/stop targets |
| `validate` | semantic findings with field paths, cross-profile port conflicts |
| `argv` | pure `ssh` argv planning and display quoting |
| `ports` | traits: process, probe, store, clock, runtime, control, terminal, open URL |
| `host` | OS implementations: process, probe, files, control, terminals, opener, ssh config |
| `manager` | stateful orchestration over the eight ports |

**Section sources**

- [crates/tunnel-core/src/lib.rs](file://crates/tunnel-core/src/lib.rs#L10-L18)

## Wire Contract

`contract.rs` mirrors the zod schemas field for field, using the wire's
camelCase so JSON stays byte-compatible with the TypeScript daemon.

- `ForwardRule` is a tagged enum on `kind` (`local`, `remote`, `dynamic`).
  Local/remote carry `bindAddress?`, `bindPort`, `targetHost`, `targetPort`;
  dynamic carries `bindAddress?`, `bindPort`.
- `TunnelMode`: `local`, `remote`, `dynamic`, `jump`, `multi`.
- `TunnelProfile`: `id`, `name`, `mode`, `sshTarget`, optional `sshPort`,
  `sshUser`, `identityFile`, `jumpHops`, required `forwards`, optional
  `extraOptions`, `serverAliveInterval`, `exitOnForwardFailure`,
  `clearAllForwardings`, `noRemoteCommand`, `remoteCommand`, `autoReconnect`,
  `notes`, plus `createdAt`/`updatedAt`.
- `TunnelStatus`: `stopped`, `starting`, `running`, `degraded`, `failed`,
  `stopping`.
- `TunnelOrigin`: `managed`, `terminal`, `external`.
- `LaunchMode`: `tunnel` (keeps `-N`) or `shell` (interactive login shell).
- `ForwardHealth`, `TunnelState`, `SshHostInfo`, `TerminalTarget`.

Optional fields use `skip_serializing_if = "Option::is_none"`, so they are
absent rather than `null` — the unit test in `contract.rs` asserts exactly that
("Optional fields stay absent, not null").

**Section sources**

- [crates/tunnel-core/src/contract.rs](file://crates/tunnel-core/src/contract.rs#L1-L170)

## Errors

The closed set is: `InvalidRequest`, `NotFound`, `Conflict`, `ResourceBusy`,
`LimitExceeded`, `SshNotFound`, `SshFailed`, `PortInUse`, `ProbeFailed`,
`StoreFailed`, `TerminalFailed`, `UnsupportedOperation`, `Timeout`,
`InternalError`.

`http_status()` is an exhaustive match: 400, 404, 409, 429, 429, 503, 502, 409,
502, 500, 502, 400, 504, 500. `TunnelError` carries `code`, `message`,
`retryable`; `retryable()` is a builder method. A unit test pins every mapping
and the serialized spelling (`"SshNotFound"`), because the TypeScript table is
the source of truth.

**Section sources**

- [crates/tunnel-core/src/error.rs](file://crates/tunnel-core/src/error.rs#L1-L95)

## State Machine

```text
stopped  → starting
starting → running | failed | stopping
running  → stopping | degraded | failed
degraded → running | stopping | failed
failed   → starting | stopped
stopping → stopped | failed
```

`start_target` allows start only from `stopped`/`failed`; `stop_target` allows
stop from `starting`/`running`/`degraded`. `can_transition` is consulted by the
manager before every status change, so an illegal move is a `Conflict`, not a
silently overwritten field.

**Section sources**

- [crates/tunnel-core/src/state_machine.rs](file://crates/tunnel-core/src/state_machine.rs#L1-L70)

## Validation

`validate_profile` returns `ProfileFinding { path, message, severity }`; only
`Error` findings block a save/start.

| Rule | Severity |
|---|---|
| blank `name` / blank `sshTarget` | error |
| no forwards / more than 16 | error |
| duplicate local bind port inside one profile (remote excluded) | error |
| more than 8 jump hops / 32 extra options / 2000-char notes | error |
| remote forward binding loopback (`""`, `localhost`, `127.0.0.1`, `::1`) | warning |
| local/dynamic bind port < 1024 | warning |
| `noRemoteCommand = false` without `remoteCommand` | warning |

`find_port_conflicts` groups non-remote bind ports across profiles and returns
ports used by more than one profile.

**Section sources**

- [crates/tunnel-core/src/validate.rs](file://crates/tunnel-core/src/validate.rs#L1-L234)

## Argv Builder

`build_ssh_argv(profile, ArgvOptions)` returns `{ argv, command }`; it performs
no I/O and does not validate. Argument order is stable and load-bearing:

1. `-o BatchMode=yes` **unless** `interactive` (an interactive shell must be
   able to answer a password prompt; ssh keeps the first value for a repeated
   option).
2. `-o StrictHostKeyChecking=accept-new`.
3. `-o ExitOnForwardFailure=yes` unless explicitly `false`.
4. `-o ClearAllForwardings=yes` when true.
5. `-o ServerAliveInterval=<n>` (default 30) and `ServerAliveCountMax=3` when
   the interval is > 0.
6. `-p <port>`, `-i <identity>`, `-J hop1,hop2` when present.
7. One `-L`/`-R`/`-D` pair per forward, in profile order.
8. Extra `-o key=value`.
9. Control trio when a socket path is supplied: `ControlMaster=yes`,
   `ControlPath=<path>`, `ControlPersist=no`.
10. Destination (`user@host` resolved by `ssh_destination`).
11. `remoteCommand` when `noRemoteCommand == false`, else `-N`; nothing when
    interactive.

`quote_for_display` produces the copyable `ssh …` line and is display-only.
The same six fixtures (`basic-local`, `remote`, `dynamic`, `jump`, `control`,
`interactive`) are asserted by Rust and TypeScript.

**Section sources**

- [crates/tunnel-core/src/argv.rs](file://crates/tunnel-core/src/argv.rs#L1-L288)
- [fixtures/argv/control.json](file://fixtures/argv/control.json#L1-L15)
- [fixtures/argv/interactive.json](file://fixtures/argv/interactive.json#L1-L15)

## Ports

| Trait | Methods | Notes |
|---|---|---|
| `ProcessPort` | `spawn`, `is_alive`, `kill_tree` | returns `LiveChild { pid, child: Option<Child> }` |
| `ProbePort` | `dial_tcp` | returns `ProbeOutcome { ok, latency_ms, detail }`, never errors on refuse |
| `StorePort` | `read_profiles`, `write_profiles` | profile persistence |
| `ClockPort` | `now_iso` | `SystemClock` uses RFC 3339 milliseconds UTC |
| `RuntimeStorePort` | `read`, `write` | `RuntimeRecord` + `RuntimeOrigin` |
| `ControlPort` | `check`, `exit` | both return bool |
| `TerminalPort` | `detect`, `launch`, `close` | launch returns the launcher pid |
| `OpenUrlPort` | `open` | scheme checked by the implementation |

`ExternalSession` is the serialized shape of a launched terminal session:
`profileId`, `terminalId`, `mode`, `command`, `pid`, `launchedAt`.

**Section sources**

- [crates/tunnel-core/src/ports.rs](file://crates/tunnel-core/src/ports.rs#L1-L139)

## Host Implementations

### Processes

`OsProcess::spawn` opens the log file in append mode, passes stdin as null,
redirects stdout/stderr to the log, and on Unix calls `process_group(0)` so the
child is detached from the app's group. A `NotFound` spawn error becomes
`SshNotFound`; anything else becomes `SshFailed`.

Liveness is `kill -0 <pid>` on Unix and `tasklist /FI "PID eq <pid>"` on
Windows. A missing `kill` binary answers "alive" deliberately: a running tunnel
should not be marked failed for lack of a probe. Kill is `kill -TERM` or
`taskkill /PID <pid> /T /F`.

### Probes and files

`TcpProbe` resolves all addresses, tries each with `connect_timeout`, and
returns latency on success or `"connection refused"` on failure. `FileStore`
reads corrupt JSON as an empty list and writes pretty JSON directly.
`FileRuntimeStore` swallows every read/write failure by contract.

### Control sockets

`SshControl` runs `ssh -o BatchMode=yes -S <socket> -O check|exit <destination>`
with a 2 s deadline, killing the child on timeout. `supports_control_master()`
is `!cfg!(windows)`.

### Terminals

Detection is per platform:

- macOS: `.app` bundle probing in `/Applications`, `/System/Applications`,
  `/System/Applications/Utilities`, `~/Applications`; targets are Terminal,
  iTerm2, Warp, Ghostty, kitty, WezTerm, Alacritty, VS Code.
- Linux: executable-on-PATH probing for GNOME Terminal, Konsole, Alacritty,
  kitty, XTerm.
- Windows: Windows Terminal and PowerShell.

Launchers differ by terminal: AppleScript for Terminal/iTerm2, `open -a` for
Warp/Ghostty, direct binaries for kitty/WezTerm/Alacritty, `code` for VS Code.
Launch waits a 700 ms settle window; a launcher that exits non-zero is a
`TerminalFailed`. Closing kills the launcher pid.

### Open URL and SSH config

`OsOpenUrl` rejects anything that is not `http://`/`https://`, then spawns
`open`, `xdg-open`, or `cmd /c start "" <url>`.

`read_ssh_config_hosts` parses `~/.ssh/config` into alias →
`{hostName?, user?, port?}`. Rules: comments/blank lines skipped, key
case-insensitive, first value wins, wildcard aliases (`*`, `?`) are excluded
from the alias list. It reads only the main file — unlike the Node
implementation, it does not process `Include`.

**Section sources**

- [crates/tunnel-core/src/host.rs](file://crates/tunnel-core/src/host.rs#L29-L150)
- [crates/tunnel-core/src/host.rs](file://crates/tunnel-core/src/host.rs#L152-L264)
- [crates/tunnel-core/src/host.rs](file://crates/tunnel-core/src/host.rs#L266-L331)
- [crates/tunnel-core/src/host.rs](file://crates/tunnel-core/src/host.rs#L333-L717)
- [crates/tunnel-core/src/host.rs](file://crates/tunnel-core/src/host.rs#L719-L884)

## Manager

`Manager<P, Q, S, C, R, T, O, K>` owns profiles, states, live children,
terminal sessions, the optional control port, log/control directories, and the
ssh path. Construction takes all eight ports plus paths; `multiplexed` is set
from `control.is_some()`.

### Load and adoption

`ensure_loaded` reads profiles once, seeds each state as `stopped`, then
`adopt_runtime`. A record survives only if `runtime_alive` witnesses it:
control socket answers, or (without multiplexing) the recorded pid is alive for
a `managed` record. Surviving records become `running`, and terminal records
recreate their `ExternalSession` with the command rebuilt from the profile.
The file is rewritten with only survivors.

### Reconciliation

`refresh_unmanaged` covers every ownership case:

- tracked session, socket alive → `running`/`terminal`;
- tracked session within the 12 s grace → `starting`/`terminal`;
- session gone but a bind port answers → `running`/`external` with
  "the terminal session ended; a listener still holds this port";
- session gone, nothing listening → `stopped`;
- managed state with no child/socket/pid → `stopped` with
  "the tunnel exited while the host was not running";
- socket alive for an untracked tunnel → `running`/`terminal`;
- `external` state whose port stopped answering → `stopped`;
- a stopped profile whose port answers → claimed as `external`.

### Start and stop

`start` checks `start_target`, sets `starting`, reclaims the control socket,
writes a `--- starting <iso> ---` log line, builds argv (with control path when
multiplexed), spawns, sleeps `900ms + 4s × hops`, reaps an early exit, and only
then sets `running` + pid + argv + origin. A failure is `failed` with
`last_error` pointing at the log; `startup_failure_hint` appends the macOS
Local Network explanation when the log tail contains `No route to host` and an
RFC1918 address. A best-effort probe follows.

`stop` is idempotent for `stopped`/`failed`, refuses `external` tunnels it does
not own, asks the control master to exit for sessions/socket-owned tunnels,
kills the child (SIGTERM via `Child::kill` or `kill_tree`), waits 1200 ms,
escalates once, removes the socket if nothing answers, writes
`--- stopped by user <iso> ---`, and persists runtime.

### Probe, log, sessions

`probe_tunnel` only probes `running`/`degraded`; it dials each local/dynamic
bind port (mapping `""`, `*`, `0.0.0.0` to `127.0.0.1`) three times at most,
marks remote forwards `ok` with an explanatory detail, then sets `degraded`
when any local probe failed.

`log_tail` reads at most 128 KiB from the end, drops a partial first line, keeps
the last `tail` lines (caller clamps 1..2000), and reports `truncated` and the
full file size. A missing log is an empty tail, not an error.

`launch_in_terminal` reclaims the socket, builds the display command (adding
the control trio when multiplexed, dropping `-N` for `shell`), launches, closes
the previous tracked session, records `ExternalSession`, sets the state to
`starting` with origin `terminal`, and persists runtime.

**Section sources**

- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L175-L367)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L398-L664)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L666-L750)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L776-L1008)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L1010-L1219)

## Tests

- Unit tests live in `contract.rs`, `error.rs`, `state_machine.rs`,
  `validate.rs`, `argv.rs`, `host.rs`, and `manager.rs`.
- `manager.rs` tests use fakes for all eight ports: create → preview → start →
  probe → stop, double-start conflict, blank-name rejection, LAN hint,
  non-LAN plain message, stale socket reclaimed, live socket never stolen.
- `tests/fixtures.rs` reads `fixtures/argv/*.json` and
  `fixtures/state-machine.json` and asserts the Rust implementation agrees
  with the same golden data the TypeScript suite uses.
- CI runs `cargo test -p tunnel-core` and
  `cargo clippy -p tunnel-core --all-targets -- -D warnings`.

**Section sources**

- [crates/tunnel-core/tests/fixtures.rs](file://crates/tunnel-core/tests/fixtures.rs#L1-L99)
- [.github/workflows/ci.yml](file://.github/workflows/ci.yml#L60-L85)

## Known Gaps

1. **No HTTP or CLI crate.** `docs/rust-core.md` plans `crates/tunnel-http`
   (axum) and `crates/tunnel-cli`; `crates/` contains only `tunnel-core`.
2. **No `Include` support in the Rust ssh-config reader**, while the Node
   reader supports one include level and globs. The desktop app therefore shows
   fewer host aliases than the daemon for configs that use `Include`.
3. **Non-atomic profile writes.** Rust uses `fs::write`; Node uses temp file +
   rename. A crash mid-write can truncate the Rust store (which then reads as
   empty).
4. **`autoReconnect` is stored and ignored**, and `mode` does not influence
   argv.
5. **Blocking sleep in `start`/`stop`.** Correct for the Tauri command pool, but
   it means one command holds the manager mutex across the grace window.
6. **`retryable` is never set by the manager.** The builder exists; only
   constructors leave it `false`.

**Section sources**

- [docs/rust-core.md](file://docs/rust-core.md#L130-L200)
- [crates/tunnel-core/src/host.rs](file://crates/tunnel-core/src/host.rs#L832-L884)
- [packages/ssh-host-node/src/ssh-config.ts](file://packages/ssh-host-node/src/ssh-config.ts#L100-L163)
- [crates/tunnel-core/src/host.rs](file://crates/tunnel-core/src/host.rs#L208-L234)
