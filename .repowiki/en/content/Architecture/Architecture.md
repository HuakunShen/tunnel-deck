# Architecture

<cite>
**Referenced Files in This Document**
- [docs/design.md](file://docs/design.md)
- [docs/rust-core.md](file://docs/rust-core.md)
- [packages/ssh-contract/src/routes.ts](file://packages/ssh-contract/src/routes.ts)
- [packages/ssh-service/src/http.ts](file://packages/ssh-service/src/http.ts)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts)
- [packages/ssh-host-node/src/ports.ts](file://packages/ssh-host-node/src/ports.ts)
- [crates/tunnel-core/src/ports.rs](file://crates/tunnel-core/src/ports.rs)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs)
- [apps/desktop/src-tauri/src/tunnel_commands.rs](file://apps/desktop/src-tauri/src/tunnel_commands.rs)
- [apps/web/src/lib/store.ts](file://apps/web/src/lib/store.ts)
- [packages/ssh-client/src/index.ts](file://packages/ssh-client/src/index.ts)
- [apps/desktop/src-tauri/tauri.conf.json](file://apps/desktop/src-tauri/tauri.conf.json)
- [integrations/dsh/src/host.ts](file://integrations/dsh/src/host.ts)
</cite>

## Table of Contents

1. [Ports and Adapters](#ports-and-adapters)
2. [Manager Semantics](#manager-semantics)
3. [Transport Selection](#transport-selection)
4. [Adoption and Ownership Reconciliation](#adoption-and-ownership-reconciliation)
5. [Timing Constants](#timing-constants)
6. [HTTP API Shape](#http-api-shape)
7. [Tauri IPC Shape](#tauri-ipc-shape)
8. [Security Boundaries](#security-boundaries)
9. [Design Tensions](#design-tensions)

## Ports and Adapters

Both managers are generic over the same seven/eight capabilities. The
TypeScript version declares them as interfaces; the Rust version as traits.
This is the seam that keeps the domain testable without spawning anything.

| Port | TypeScript interface | Rust trait | Real implementation |
|---|---|---|---|
| process spawn/kill | `ProcessPort` | `ProcessPort` | detached `ssh`, log file append |
| TCP probe | `ProbePort` | `ProbePort` | `net.connect` / `TcpStream::connect_timeout` |
| profile store | `StorePort` | `StorePort` | `profiles.json` (atomic in Node, plain write in Rust) |
| clock | `ClockPort` | `ClockPort` | `new Date()` / `chrono::Utc` |
| terminal | `TerminalPort` | `TerminalPort` | per-OS launcher + detection |
| open URL | `OpenUrlPort` | `OpenUrlPort` | `open` / `xdg-open` / `cmd start` |
| control socket | `ControlPort` | `ControlPort` | `ssh -O check|exit` |
| runtime records | `RuntimeStorePort` | `RuntimeStorePort` | `runtime.json` |

`ssh-service` imports `isOpenableUrl` from the host layer so URL validation is
shared between the manager and the OS call. `tunnel-core` keeps the same
allow-list inside `OsOpenUrl::open`.

**Section sources**

- [packages/ssh-host-node/src/ports.ts](file://packages/ssh-host-node/src/ports.ts#L1-L150)
- [crates/tunnel-core/src/ports.rs](file://crates/tunnel-core/src/ports.rs#L1-L139)
- [crates/tunnel-core/src/host.rs](file://crates/tunnel-core/src/host.rs#L719-L755)

## Manager Semantics

The two managers mirror each other method for method:

| Operation | TypeScript | Rust |
|---|---|---|
| list/get profile | `listProfiles`, `getProfile` | `list_profiles`, `get_profile` |
| create/update/delete | stamps id + timestamps, refuses edits while live | same |
| list/get state | reconciles first (`refreshUnmanaged`) | same |
| start | state machine + spawn + grace + probe | same |
| stop | control exit or kill, then escalate | same |
| stop-all | continues past one failure | same |
| probe | TCP dial + 2 retries, marks `degraded` | same |
| log tail | last 128 KiB, drop partial first line | same |
| terminal handoff | build command with control socket, launch, track pid | same |
| preview | argv + display command, no I/O | same |

Limits are duplicated: 200 profiles, 16 forwards, 8 jump hops, 32 extra
options, 2000-char notes, 128 KiB log window.

The important implementation difference is concurrency. Rust `Manager` is
blocking and owned behind a `Mutex` in the Tauri state; each command locks it
(`state.manager.lock()`), so `start`'s grace sleep does not block the UI thread
but does serialize other commands. TypeScript `TunnelManager` is async and
relies on the single-threaded event loop.

**Section sources**

- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L113-L260)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L175-L275)
- [crates/tunnel-core/src/validate.rs](file://crates/tunnel-core/src/validate.rs#L1-L60)

## Transport Selection

The Svelte console is transport-agnostic because the client interface is the
same. `getClient()` chooses once and caches:

1. `window.__TAURI_INTERNALS__.invoke` exists → `createInvokeTunnelClient`.
2. Page protocol is `vscode-webview:` → `http://127.0.0.1:7432`.
3. `?daemon=URL` query parameter → that base URL (dev against a running daemon).
4. Otherwise → relative URLs (daemon serves the SPA from the same origin).

Tauri exposes no `withGlobalTauri`, so only the internals object is available;
the CSP allows `connect-src 'self' ipc: http://ipc.localhost` and no loopback
HTTP origin.

**Section sources**

- [apps/web/src/lib/store.ts](file://apps/web/src/lib/store.ts#L28-L82)
- [apps/desktop/src-tauri/tauri.conf.json](file://apps/desktop/src-tauri/tauri.conf.json#L1-L40)

## Adoption and Ownership Reconciliation

`ensureLoaded` reads profiles once, seeds every state as `stopped`, then calls
`adoptRuntime`. A record is a claim, not proof: each entry must be witnessed by
something that still answers.

```text
read runtime.json
  for each record with a matching profile:
      POSIX: ssh -O check on control/<id>.sock
      no multiplexing: recorded pid is alive
      if alive -> state=running, origin from record, keep record
      if dead  -> drop record
write back the records that survived
```

On every `listStates`/`getState`, `refreshUnmanaged` also handles:

- tracked terminal session whose socket answers → `running`;
- tracked session still inside the 12 s grace → `starting`;
- session gone but its bind port still answers → `running` with
  `origin=external` and an explanatory `lastError`;
- session gone and no listener → `stopped`;
- managed tunnel with no live child/socket/pid → `stopped` with
  "the tunnel exited while the host was not running";
- otherwise, a stopped profile whose bind port answers is claimed as
  `external` ("started outside this host").

The Rust version additionally reclaims a stale control socket before a new
start or terminal handoff: if nothing answers behind it, the file is removed;
if something answers, the operation fails with `Conflict` rather than stealing
a live session.

**Section sources**

- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L154-L236)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L476-L577)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L252-L337)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L410-L445)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L483-L616)

## Timing Constants

| Constant | Value | Where | Why |
|---|---|---|---|
| start grace | `900ms + 4s × jump hops` | both managers | `ExitOnForwardFailure` kills bad forwards early; each jump adds a handshake |
| terminal start grace | 12 000 ms | both managers | a handed-off window must authenticate before its socket is judged dead |
| stop escalation | 1200 ms then 400 ms | both managers | SIGTERM, then kill if the pid still answers |
| terminal launch settle | 700 ms | both hosts | a launcher that dies instantly is the only observable failure |
| control command timeout | 2 000 ms | both hosts | a wedged socket must not hang a status poll |
| probe dial timeout | 2500 ms × 3 attempts, 800 ms apart | both managers | listeners can lag process liveness |
| log window | 128 KiB | both managers | a long-running daemon must not read megabytes for a dialog |

**Section sources**

- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L70-L75)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L26-L31)
- [packages/ssh-host-node/src/terminal.ts](file://packages/ssh-host-node/src/terminal.ts#L300-L310)
- [packages/ssh-host-node/src/control.ts](file://packages/ssh-host-node/src/control.ts#L20-L25)

## HTTP API Shape

`createServiceApp` registers 19 handlers. Requests are validated with the
contract's `strictObject` schemas; responses are validated again with the same
schema before serialization ("shape drifted" becomes `InternalError`). Errors
become `{ problem: { code, message, retryable, details? } }` with the status
from `TUNNEL_STATUS_BY_CODE`.

The declared route table (`TUNNEL_ROUTES`) has 18 entries and omits the session
close route (`DELETE /api/profiles/:id/session`), which exists in `http.ts` and
in the typed client. See [Boundaries and Drift](Boundaries%20and%20Drift.md).

**Section sources**

- [packages/ssh-service/src/http.ts](file://packages/ssh-service/src/http.ts#L1-L120)
- [packages/ssh-contract/src/routes.ts](file://packages/ssh-contract/src/routes.ts#L1-L60)

## Tauri IPC Shape

`tunnel_commands.rs` registers 19 commands whose names map 1:1 to routes
(`tunnel_list_profiles` ↔ `GET /api/profiles`). Commands are synchronous: the
blocking manager runs on Tauri's command thread pool. Responses are serialized
with `rename_all = "camelCase"` so the shared client's zod schemas validate IPC
payloads unchanged. Failures serialize `CommandProblem { code, message, retryable }`.

Two asymmetries are real:

- `tunnel_close_session` returns `Ok(bool)` (false when nothing was tracked),
  while HTTP returns `404 NotFound`. The client ignores the body in both cases,
  so only the HTTP transport surfaces an error.
- `tunnel_health.version` is `CARGO_PKG_VERSION` (`0.1.0`) while the daemon
  reports `SERVICE_VERSION` (`0.1.0`) — same value today, different sources.

**Section sources**

- [apps/desktop/src-tauri/src/tunnel_commands.rs](file://apps/desktop/src-tauri/src/tunnel_commands.rs#L73-L225)
- [apps/desktop/src-tauri/src/tunnel_commands.rs](file://apps/desktop/src-tauri/src/tunnel_commands.rs#L376-L387)
- [packages/ssh-client/src/index.ts](file://packages/ssh-client/src/index.ts#L150-L316)

## Security Boundaries

- **Loopback by default**: daemon binds `127.0.0.1:7432`; CORS is enabled only
  for `/api/*`, which is acceptable because the server is loopback-only.
- **Loopback guard in the DSH plugin**: Host and Origin must be
  `127.0.0.1:<harness port>` or `localhost:<port>`, `origin: "null"` is refused,
  and an origin-less `Sec-Fetch-Site: cross-site` request is refused.
- **Scheme allow-list**: only `http://` and `https://` are handed to the OS
  opener; everything else is rejected before it reaches a command line.
- **No shell**: every spawn passes argv directly (`Command::new(program).args`,
  `spawn(file, args)`), including AppleScript payloads, which are escaped as
  literals rather than interpolated into a shell string.
- **CSP in the desktop app**: `default-src 'self'`, no remote origins, frames
  refused.
- **macOS Local Network**: `Info.plist` carries
  `NSLocalNetworkUsageDescription`; the Rust manager turns `No route to host`
  for an RFC1918 address into an explicit privacy-grant hint.

**Section sources**

- [packages/ssh-service/src/serve.ts](file://packages/ssh-service/src/serve.ts#L88-L100)
- [integrations/dsh/src/host.ts](file://integrations/dsh/src/host.ts#L120-L185)
- [packages/ssh-host-node/src/open-url.ts](file://packages/ssh-host-node/src/open-url.ts#L1-L40)
- [apps/desktop/src-tauri/tauri.conf.json](file://apps/desktop/src-tauri/tauri.conf.json#L22-L30)
- [apps/desktop/src-tauri/Info.plist](file://apps/desktop/src-tauri/Info.plist#L1-L17)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L81-L114)

## Design Tensions

1. **Two managers, one contract.** Every domain change must land twice, or the
   Rust desktop app and the Node daemon drift. `fixtures/` only covers argv and
   state transitions; persistence, adoption, probing and log reading have no
   cross-language fixtures.
2. **The daemon remains the richest backend.** Terminal launch, open-URL,
   runtime adoption and stale-socket reclaim exist in both, but the CLI
   `--direct` path and the DSH plugin construct managers with fewer ports, so
   their capabilities differ from the daemon's.
3. **`mode` is metadata.** `buildSshArgv`/`build_ssh_argv` never read
   `profile.mode`; the flags come from `forwards` and `jumpHops`. `mode` is
   displayed and stored only.
4. **The Phase 2/3 plan is not code.** `docs/rust-core.md` describes
   `tunnel-http` and `tunnel-cli` crates and a `ssh-tunnel serve` binary; none
   exist in `crates/`.

**Section sources**

- [docs/rust-core.md](file://docs/rust-core.md#L130-L200)
- [packages/ssh-core/src/argv.ts](file://packages/ssh-core/src/argv.ts#L85-L170)
- [crates/tunnel-core/src/argv.rs](file://crates/tunnel-core/src/argv.rs#L150-L280)
