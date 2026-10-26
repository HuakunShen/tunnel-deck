# TypeScript Service Layer

<cite>
**Referenced Files in This Document**
- [packages/ssh-service/package.json](file://packages/ssh-service/package.json)
- [packages/ssh-service/src/index.ts](file://packages/ssh-service/src/index.ts)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts)
- [packages/ssh-service/src/http.ts](file://packages/ssh-service/src/http.ts)
- [packages/ssh-service/src/serve.ts](file://packages/ssh-service/src/serve.ts)
- [packages/ssh-host-node/src/ports.ts](file://packages/ssh-host-node/src/ports.ts)
- [packages/ssh-host-node/src/runtime-store.ts](file://packages/ssh-host-node/src/runtime-store.ts)
- [packages/ssh-contract/src/routes.ts](file://packages/ssh-contract/src/routes.ts)
- [packages/ssh-client/src/index.ts](file://packages/ssh-client/src/index.ts)
- [tests/contract/http.test.ts](file://tests/contract/http.test.ts)
- [tests/contract/multiplex.test.ts](file://tests/contract/multiplex.test.ts)
</cite>

## Table of Contents

1. [Composition Root](#composition-root)
2. [TunnelManager](#tunnelmanager)
3. [Lifecycle Operations](#lifecycle-operations)
4. [Terminal Sessions and URL Opening](#terminal-sessions-and-url-opening)
5. [HTTP Application](#http-application)
6. [Daemon Launcher](#daemon-launcher)
7. [Persistence Semantics](#persistence-semantics)
8. [Limits and Timing](#limits-and-timing)
9. [Tests](#tests)
10. [Gaps and Asymmetries](#gaps-and-asymmetries)

## Composition Root

`@ssh-tunnel/service` is the Node composition root: it exports the manager, the
Hono app factory, and `startDaemon`. It depends on `ssh-contract`, `ssh-core`,
`ssh-host-node`, `hono`, and `@hono/node-server`. It does not import
`ssh-client`; clients are consumers, not dependencies.

```ts
// pods assembled by the caller
new TunnelManager({
  process, probe, store, clock, logDir,
  sshPath?, maxProfiles?, terminal?, openUrl?, control?, controlDir?, runtime?,
});
```

Everything except `process`, `probe`, `store`, `clock` and `logDir` is
optional. The shape of that optionality is the capability model:

- no `terminal` → `listTerminals()` is empty and terminal launch is impossible;
- no `openUrl` → opening a URL raises `UnsupportedOperation`;
- no `control` → `multiplexed = false`, so the manager owns tunnels by pid;
- no `runtime` → no adoption across restarts.

`apps/cli --direct` and `apps/tui --direct` construct a manager with only the
required five, which is why their capabilities are narrower than the daemon's.

**Section sources**

- [packages/ssh-service/src/index.ts](file://packages/ssh-service/src/index.ts#L1-L10)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L46-L153)
- [packages/ssh-service/src/serve.ts](file://packages/ssh-service/src/serve.ts#L40-L80)

## TunnelManager

State is held in five maps: `profiles`, `states`, `live`, `sessions`, plus a
`loaded` flag. `ensureLoaded` reads the profile store once, seeds
`{status:"stopped", forwards:[]}` for each, then adopts runtime records.

Profile operations:

| Method | Behaviour |
|---|---|
| `listProfiles` | sorted by `name.localeCompare` |
| `getProfile` | `NotFound { profileId }` when absent |
| `createProfile` | re-parses the omitted schema, runs semantic validation, stamps `t-<base36 time>-<random>`, timestamps, persists |
| `updateProfile` | refuses while live/starting, preserves `createdAt`, revalidates, persists |
| `deleteProfile` | refuses unless stopped/failed, closes a tracked session, deletes state, persists both stores |

The profile id format is `t-${Date.now().toString(36)}-${random}`. Rust uses
`t-<base36 millis>-<timestamp-derived suffix>`; both are URL-safe.

**Section sources**

- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L154-L355)

## Lifecycle Operations

`start(id)`:

1. `refreshState` (a live child that died becomes `failed`).
2. `startTarget` must be non-null, else `Conflict`.
3. set `starting`, clearing `lastError`.
4. `mkdir` the control directory when multiplexed (mode `0o700`).
5. build argv with `handoffOptions(id)` (control path when multiplexed).
6. spawn detached `ssh` with the log file.
7. attach `child.exited.then(...)`: an exit while `starting`/`running`/
   `degraded` becomes `failed` with the exit code and log path; exits during a
   stop are ignored.
8. sleep `900 + 4000 × hops` ms.
9. if the child is gone, set `failed` and return; otherwise set `running` with
   `origin`, `pid`, `startedAt`, `argv`.
10. persist runtime, then fire `probeTunnel` without awaiting it.

`stop(id)`:

1. refresh; if `stopped`/`failed`, return as-is (idempotent).
2. `stopTarget` must be non-null, else `Conflict`.
3. refuse a tunnel that is `external` and has no session/socket/child.
4. set `stopping`.
5. for a session or socket-owned tunnel, `control.exit(socket, destination)`
   and drop the session.
6. for a live child, `killTree` (SIGTERM), wait 1200 ms, then `SIGKILL` if still
   alive, wait 400 ms.
7. without multiplexing, kill the recorded pid of an adopted managed tunnel.
8. set `stopped`, clear pid/startedAt, persist runtime.

`stopAll` iterates all ids and swallows individual failures so one stuck tunnel
cannot block the rest.

**Section sources**

- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L578-L784)

## Terminal Sessions and URL Opening

`launchInTerminal(id, terminalId, mode)` builds the display command with a
control path when multiplexed and `interactive: mode === "shell"`, calls
`terminal.launch`, rejects a zero pid, closes the previously tracked session,
and stores `ExternalSession`. The state becomes `starting` with origin
`terminal` and no pid (the window's ssh is not our child). Health remains
port-probe based, so a user-closed window is `stopped`, never `failed`.

`closeSession(id)` removes the session, asks the control master to exit, closes
the launcher pid, persists runtime, and returns whether a session existed.

`openUrl(url)` refuses when no `OpenUrlPort` is wired
(`UnsupportedOperation`), validates with `isOpenableUrl`, calls the port, and
returns `{ url, openedAt }`.

**Section sources**

- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L914-L1062)
- [packages/ssh-host-node/src/open-url.ts](file://packages/ssh-host-node/src/open-url.ts#L19-L32)

## HTTP Application

`createServiceApp({ manager, serviceInstanceId, webDir? })` returns a Hono app:

- `app.use("/api/*", cors())`;
- 19 handlers covering profiles, preview, log, states, start/stop/stop-all/
  probe, ssh hosts, terminals, launch, session close, open-url, health;
- each mutating handler parses JSON, validates with a contract schema, maps
  `TunnelError` through `TUNNEL_STATUS_BY_CODE`, and maps unknown errors to
  `InternalError`;
- responses are re-validated with the response schema before `c.json`, so a
  shape regression becomes a problem response rather than a drifted payload;
- `/api/health` returns `{ ok: true, version: SERVICE_VERSION, serviceInstanceId }`.

`tail` is clamped to `1..2000` and defaults to 200. Create returns `201`.
Delete and session-close return `{ ok: true }` on success.

**Section sources**

- [packages/ssh-service/src/http.ts](file://packages/ssh-service/src/http.ts#L1-L297)
- [packages/ssh-contract/src/routes.ts](file://packages/ssh-contract/src/routes.ts#L1-L35)

## Daemon Launcher

`startDaemon(options)`:

- resolves `~/.ssh-tunnel-ui` (or `HOME`/`USERPROFILE`/cwd) for profiles, logs,
  runtime and `daemon.json`;
- only creates directories it will use, so an explicit `--store`/`--log-dir`
  elsewhere does not force a home write;
- wires the real ports: `createNodeProcessPort`, `createNodeProbePort`,
  `createFileStorePort`, `systemClock`, `createTerminalPort`,
  `createOpenUrlPort`, `createControlPort` only when
  `supportsControlMaster(platform)`, and `createFileRuntimeStore`;
- builds `instanceId = svc-<base36 time>-<random>`;
- serves the API, optionally the static Web UI at `/*` with an SPA fallback to
  `index.html` and a `..` traversal check;
- binds `127.0.0.1:7432` by default and writes `daemon.json`
  (`{port, host, instanceId, version, pid}`) best-effort.

The static branch reads whole files with `readFileSync` per request. That is
acceptable for a local console but has no caching or ETag.

**Section sources**

- [packages/ssh-service/src/serve.ts](file://packages/ssh-service/src/serve.ts#L1-L108)

## Persistence Semantics

Profiles and runtime records are deliberately different:

| Aspect | Profiles | Runtime |
|---|---|---|
| File | `profiles.json` | `runtime.json` |
| Write | temp file + `rename`, 0644 | temp file + `rename`, mode `0600` |
| Read failure | `StoreFailed` with path | `[]` |
| Corrupt JSON | `StoreFailed` | `[]` |
| Invalid entries | skipped individually | invalid records dropped |
| Purpose | user configuration | disposable ownership claim |

Runtime records are written on start, stop, launch, close and session reap;
they are read once during adoption. Records only exist for `managed`/`terminal`
origins in `running`/`degraded`/`starting`.

**Section sources**

- [packages/ssh-host-node/src/store.ts](file://packages/ssh-host-node/src/store.ts#L1-L69)
- [packages/ssh-host-node/src/runtime-store.ts](file://packages/ssh-host-node/src/runtime-store.ts#L1-L92)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L214-L236)

## Limits and Timing

| Limit | Value |
|---|---|
| profiles | 200 (`maxProfiles`, default 200) |
| forwards per profile | 16 (schema + validation) |
| jump hops | 8 |
| extra options | 32 |
| notes | 2000 chars |
| log tail window | 128 KiB |
| log tail lines | 1..2000 |
| start grace | `900 + 4000 × hops` ms |
| terminal start grace | 12 000 ms |
| stop escalation | 1200 ms + 400 ms |
| probe | 2500 ms, up to 3 attempts, 800 ms apart |
| control command | 2000 ms |

**Section sources**

- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L70-L75)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L785-L850)
- [packages/ssh-contract/src/tunnel.ts](file://packages/ssh-contract/src/tunnel.ts#L80-L115)

## Tests

`tests/contract/http.test.ts` drives the Hono app against an in-memory manager
with fake ports. It covers CRUD, validation failures, start/stop, probe,
stop-all, logs, terminals, launch, open-url, ssh-host inventory, and error
status mapping.

`tests/contract/multiplex.test.ts` pins the two halves of the Windows decision:
argv without control options, and adoption by pid after a restart.
`tests/contract/runtime-store.test.ts` covers every runtime-file degradation.

**Section sources**

- [tests/contract/http.test.ts](file://tests/contract/http.test.ts#L1-L60)
- [tests/contract/multiplex.test.ts](file://tests/contract/multiplex.test.ts#L1-L40)
- [tests/contract/runtime-store.test.ts](file://tests/contract/runtime-store.test.ts#L1-L40)

## Gaps and Asymmetries

1. **`start` returns before the first probe.** It schedules
   `void this.probeTunnel(id)`; Rust `start` awaits `probe_tunnel` and returns
   the probed state. A front that reads `forwards` immediately sees `[]` from
   the daemon and populated health from the app.
2. **No stale-socket reclaim.** Only the Rust manager removes a dead
   control-socket file before start/handoff.
3. **No log run boundaries.** Only Rust writes start/stop marker lines.
4. **`preview(id)` does not call `ensureLoaded`.** It reads the private map
   directly; on a cold process a preview can be `NotFound` while the HTTP
   route would otherwise have listed profiles. In practice list/create calls
   load first.
5. **`refreshState` only fails unexpected exits for `managed` tunnels with a
   live child**; adopted or terminal tunnels rely on reconciliation passes.
6. **`deleteProfile` calls `closeSession` synchronously** and ignores its
   boolean; the HTTP layer then reports success either way.

**Section sources**

- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L670-L680)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L372-L400)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L894-L906)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L334-L350)
