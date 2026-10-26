# Node Host

<cite>
**Referenced Files in This Document**
- [packages/ssh-host-node/package.json](file://packages/ssh-host-node/package.json)
- [packages/ssh-host-node/src/index.ts](file://packages/ssh-host-node/src/index.ts)
- [packages/ssh-host-node/src/ports.ts](file://packages/ssh-host-node/src/ports.ts)
- [packages/ssh-host-node/src/process.ts](file://packages/ssh-host-node/src/process.ts)
- [packages/ssh-host-node/src/probe.ts](file://packages/ssh-host-node/src/probe.ts)
- [packages/ssh-host-node/src/store.ts](file://packages/ssh-host-node/src/store.ts)
- [packages/ssh-host-node/src/runtime-store.ts](file://packages/ssh-host-node/src/runtime-store.ts)
- [packages/ssh-host-node/src/control.ts](file://packages/ssh-host-node/src/control.ts)
- [packages/ssh-host-node/src/which.ts](file://packages/ssh-host-node/src/which.ts)
- [packages/ssh-host-node/src/ssh-config.ts](file://packages/ssh-host-node/src/ssh-config.ts)
- [packages/ssh-host-node/src/terminal.ts](file://packages/ssh-host-node/src/terminal.ts)
- [packages/ssh-host-node/src/open-url.ts](file://packages/ssh-host-node/src/open-url.ts)
- [tests/contract/which.test.ts](file://tests/contract/which.test.ts)
- [tests/contract/runtime-store.test.ts](file://tests/contract/runtime-store.test.ts)
</cite>

## Table of Contents

1. [Purpose](#purpose)
2. [Port Interfaces](#port-interfaces)
3. [Processes](#processes)
4. [Probing](#probing)
5. [Profile Store](#profile-store)
6. [Runtime Store](#runtime-store)
7. [Control Sockets](#control-sockets)
8. [PATH Resolution](#path-resolution)
9. [SSH Config Inventory](#ssh-config-inventory)
10. [Terminal Discovery and Launch](#terminal-discovery-and-launch)
11. [Opening URLs](#opening-urls)
12. [Tests and Gaps](#tests-and-gaps)

## Purpose

`@ssh-tunnel/host-node` is the trusted Node host: the only TypeScript package
allowed to touch `node:*`. It implements the ports that `ssh-core` and
`ssh-service` depend on, and it "parses no wire input" — validation stays in the
contract and manager layers.

**Section sources**

- [packages/ssh-host-node/src/index.ts](file://packages/ssh-host-node/src/index.ts#L1-L20)
- [packages/ssh-host-node/src/ports.ts](file://packages/ssh-host-node/src/ports.ts#L1-L20)

## Port Interfaces

`ports.ts` declares `SpawnedProcess`, `ProcessPort`, `ProbePort`, `StorePort`,
`ClockPort`, `TerminalTargetInfo`, `TerminalPort`, `RuntimeRecord`,
`RuntimeStorePort`, `ControlPort`, `OpenUrlPort`, and the `systemClock`
singleton.

Two comments encode design decisions that matter operationally:

- `TerminalPort.launch` "deliberately waits a short settle window, because a
  launcher that dies instantly is the only observable failure".
- `RuntimeRecord` is "the much smaller, disposable record of what was actually
  up", not configuration.

**Section sources**

- [packages/ssh-host-node/src/ports.ts](file://packages/ssh-host-node/src/ports.ts#L1-L150)

## Processes

`createNodeProcessPort(sshPath = "ssh")`:

- opens the log file in append mode for stdout and stderr (falling back to
  `"ignore"` if the open fails);
- spawns `ssh` with `detached: true`, `stdio: ["ignore", outFd, errFd]`, and no
  shell;
- wraps the child's `exit` and `error` events into an `exited` promise (error
  resolves `{code:127}`);
- calls `child.unref()` so the daemon can exit without killing tunnels;
- `isAlive` uses `process.kill(pid, 0)`; `killTree` sends `SIGTERM` only.

There is no tree kill beyond SIGTERM, by design: "`ssh -N` has no
grandchildren". Escalation is the manager's job (SIGKILL after 1200 ms).

**Section sources**

- [packages/ssh-host-node/src/process.ts](file://packages/ssh-host-node/src/process.ts#L1-L73)

## Probing

`createNodeProbePort().dialTcp(host, port, timeoutMs)` uses `net.connect`, sets a
socket timeout, and settles once:

- `connect` → `{ ok: true, latencyMs }`;
- `timeout` → `{ ok: false, detail: "dial host:port timed out after Nms" }`;
- `error` → `{ ok: false, detail: "dial host:port: <message>" }`.

It never rejects, which is what lets `probeTunnel` treat a refused port as a
health fact rather than an exception.

**Section sources**

- [packages/ssh-host-node/src/probe.ts](file://packages/ssh-host-node/src/probe.ts#L1-L50)

## Profile Store

`createFileStorePort(storePath)` defaults to
`$HOME/.ssh-tunnel-ui/profiles.json` (`USERPROFILE`, then cwd as fallbacks).

- `readProfiles`: `ENOENT` → `[]`; other read errors → `StoreFailed` with
  `{path}`; invalid JSON → `StoreFailed "store JSON is corrupt at …"`;
  non-array → `StoreFailed`; each entry is validated with
  `tunnelProfileSchema` and invalid entries are skipped (not fatal).
- `writeProfiles`: `mkdir -p`, write `<path>.<pid>.tmp`, `rename` over the
  target. Any failure → `StoreFailed`.
- The returned object also exposes `path`, which the daemon does not currently
  use but tests can.

**Section sources**

- [packages/ssh-host-node/src/store.ts](file://packages/ssh-host-node/src/store.ts#L1-L69)

## Runtime Store

`createFileRuntimeStore(path)` defaults to `$HOME/.ssh-tunnel-ui/runtime.json`.
The contract is the opposite of the profile store:

- every read failure (missing, unreadable, corrupt, non-array) → `[]`;
- `parseRecords` keeps an entry only when `profileId` is a non-empty string,
  `origin` is `managed|terminal`, `startedAt` is a non-empty string; `pid` must
  be a positive integer, `mode` must be `tunnel|shell`, `terminalId` a
  non-empty string;
- writes go through temp file + rename with mode `0o600`;
- write failures are swallowed. "A read-only home must not stop a tunnel."

**Section sources**

- [packages/ssh-host-node/src/runtime-store.ts](file://packages/ssh-host-node/src/runtime-store.ts#L1-L92)

## Control Sockets

`supportsControlMaster(platform = process.platform)` returns `platform !== "win32"`.
Callers use it before adding control options, because Windows OpenSSH would
accept the flag but create no socket.

`createControlPort(sshPath = "ssh", timeoutMs = 2000)` spawns
`ssh -o BatchMode=yes -S <socket> -O check|exit <destination>` with
`stdio: "ignore"`. A timeout kills the child with `SIGKILL` and resolves
`false`; spawn errors resolve `false`; exit code 0 resolves `true`.

**Section sources**

- [packages/ssh-host-node/src/control.ts](file://packages/ssh-host-node/src/control.ts#L1-L79)

## PATH Resolution

`which.ts` exists because Windows separates `PATH` with `;` and appends
extensions from `PATHEXT`. `executableNames(command, platform, pathext)` returns
`[command]` on POSIX, and on Windows the extension variants unless the command
already ends with a known extension. `whichOnPath(command, options)` splits on
`platform === "win32" ? ";" : ":"`, skips empty entries (so the current
directory is never searched), and returns the first file that is a regular file
and `X_OK` (existence on Windows).

The options allow a host platform, PATH value and PATHEXT to be injected, which
is how the Windows branch is tested from macOS.

**Section sources**

- [packages/ssh-host-node/src/which.ts](file://packages/ssh-host-node/src/which.ts#L1-L87)
- [tests/contract/which.test.ts](file://tests/contract/which.test.ts#L1-L30)

## SSH Config Inventory

`readSshConfigHosts(home)` is read-only and never throws:

1. read `~/.ssh/config`; missing → `[]`;
2. scan `Include` directives, expand `~`, relative paths and absolute paths,
   and read one level of includes with a single `*` glob in the basename;
3. parse blocks: `Host` starts a block, `HostName`/`User`/`Port` take the first
   value seen;
4. collect aliases that are not wildcards, de-duplicate, sort with
   `localeCompare`, and resolve each alias against all blocks using wildcard
   matching (`*`, `?`) in file order.

The Rust reader implements steps 1, 3 and 4 but not `Include`, so the desktop
app can show a shorter host list than the daemon.

**Section sources**

- [packages/ssh-host-node/src/ssh-config.ts](file://packages/ssh-host-node/src/ssh-config.ts#L1-L163)
- [crates/tunnel-core/src/host.rs](file://crates/tunnel-core/src/host.rs#L832-L884)

## Terminal Discovery and Launch

`detectTerminals()` probes the filesystem only:

- macOS: app bundles in the four application directories; the bundle is the
  installation signal, and the executable inside `Contents/MacOS` is discovered
  by listing rather than assuming the app name.
- Linux: `whichOnPath` for each known launcher.
- Windows: `wt.exe`, `powershell.exe`.

`createTerminalPort().launch(terminalId, commandLine)`:

1. rejects an unknown terminal with `NotFound`;
2. opens a temp launcher log (`ssh-tunnel-launch-<pid>-<time>.log`);
3. spawns the per-terminal launcher (AppleScript for Terminal/iTerm2, `open -a`
   for Warp, direct binaries elsewhere);
4. waits `LAUNCH_SETTLE_MS = 700` for an immediate exit;
5. a non-zero exit rejects with the launcher's own bounded stderr (for example
   iTerm2 refusing Apple events); a surviving or zero-exit launcher resolves its
   pid.

`close(pid)` sends SIGTERM. Health never depends on the launcher process; it is
always the forwarded port probe.

**Section sources**

- [packages/ssh-host-node/src/terminal.ts](file://packages/ssh-host-node/src/terminal.ts#L234-L406)

## Opening URLs

`createOpenUrlPort()` refuses anything not matching `^https?://` (the
allow-list is "the security boundary, not a nicety"), then spawns `open` on
macOS, `cmd.exe /c start "" <url>` on Windows, or `xdg-open` elsewhere with
`detached: true`, `stdio: "ignore"`, and `unref()`. The URL is its own argv
entry, so no shell quoting is involved.

`isOpenableUrl` is exported and reused by `TunnelManager.openUrl`, so the
manager and the OS seam agree on what is allowed.

**Section sources**

- [packages/ssh-host-node/src/open-url.ts](file://packages/ssh-host-node/src/open-url.ts#L1-L60)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L1032-L1048)

## Tests and Gaps

- `tests/contract/which.test.ts` covers Windows/POSIX name resolution, PATHEXT,
  empty PATH entries and executable checks using temp directories.
- `tests/contract/runtime-store.test.ts` covers corruption, invalid entries,
  write failures and file mode.
- Process, probe, control, terminal and open-url are exercised indirectly
  through `tests/contract/http.test.ts` with fake ports, not against the real
  OS. There is no test that launches a real terminal or a real `ssh`.

Gaps worth knowing:

1. Terminal detection and launch are macOS-shaped; Linux/Windows branches have
   no dedicated tests.
2. `open()`'s `child.on("error", ...)` rethrows inside the event handler, which
   surfaces as an uncaught exception rather than a rejected promise.
3. `store.ts` skips a profile that fails schema validation without reporting
   it, so a drifted profile silently disappears from the list.
4. `readSshConfigHosts` resolves includes but does not guard against include
   cycles; it reads only one level, which bounds the damage.

**Section sources**

- [tests/contract/runtime-store.test.ts](file://tests/contract/runtime-store.test.ts#L1-L67)
- [tests/contract/http.test.ts](file://tests/contract/http.test.ts#L1-L40)
- [packages/ssh-host-node/src/open-url.ts](file://packages/ssh-host-node/src/open-url.ts#L45-L60)
