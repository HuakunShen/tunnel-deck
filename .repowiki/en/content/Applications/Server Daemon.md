# Server Daemon

<cite>
**Referenced Files in This Document**
- [apps/server/package.json](file://apps/server/package.json)
- [apps/server/src/server.ts](file://apps/server/src/server.ts)
- [apps/server/scripts/build.ts](file://apps/server/scripts/build.ts)
- [packages/ssh-service/src/serve.ts](file://packages/ssh-service/src/serve.ts)
- [packages/ssh-service/src/http.ts](file://packages/ssh-service/src/http.ts)
- [packages/ssh-host-node/src/process.ts](file://packages/ssh-host-node/src/process.ts)
- [README.md](file://README.md)
</cite>

## Table of Contents

1. [Binary and Composition](#binary-and-composition)
2. [Flags](#flags)
3. [Startup Sequence](#startup-sequence)
4. [Static Web UI](#static-web-ui)
5. [Discovery File](#discovery-file)
6. [Operational Behaviour](#operational-behaviour)
7. [Gaps](#gaps)

## Binary and Composition

`apps/server` publishes `ssh-tunnel-serve` → `dist/server.mjs`. It depends only
on `@ssh-tunnel/service`; all wiring lives in `startDaemon`, and `server.ts`
contains argv parsing and one log line. That keeps the daemon replaceable: the
DSH plugin and tests call the same `startDaemon`/`createServiceApp` without the
binary.

The bundle is esbuild ESM with no externals, banner `#!/usr/bin/env node`, and
`process.env.NODE_ENV` defined as production.

**Section sources**

- [apps/server/package.json](file://apps/server/package.json#L1-L25)
- [apps/server/src/server.ts](file://apps/server/src/server.ts#L1-L15)
- [apps/server/scripts/build.ts](file://apps/server/scripts/build.ts#L1-L20)

## Flags

| Flag | Default | Validation |
|---|---|---|
| `--port=N` | 7432 | integer 1..65535, else exit 1 |
| `--host=H` | 127.0.0.1 | none |
| `--store=PATH` | `~/.ssh-tunnel-ui/profiles.json` | none |
| `--log-dir=DIR` | `~/.ssh-tunnel-ui/logs` | none |
| `--web-dir=DIR` | unset | serves static UI when set |
| `--ssh-path=PATH` | `ssh` | passed to process/control ports |
| `--help` / `-h` | — | prints usage and exits |

Flags are `--name=value` only; there is no space-separated form and no short
option besides `-h`. `--store` also determines where `runtime.json` and
`daemon.json` live (same directory as the store).

**Section sources**

- [apps/server/src/server.ts](file://apps/server/src/server.ts#L1-L75)
- [packages/ssh-service/src/serve.ts](file://packages/ssh-service/src/serve.ts#L14-L60)

## Startup Sequence

`startDaemon`:

1. resolves `HOME`/`USERPROFILE`/cwd and `~/.ssh-tunnel-ui`;
2. computes store, log dir, runtime path; creates only the directories it uses
   (an explicit path elsewhere must not force a home write);
3. builds `TunnelManager` with the real ports, including `control` only when
   `supportsControlMaster(process.platform)`;
4. creates `instanceId = svc-<base36 time>-<random>`;
5. builds the Hono app, optionally adds the static `/*` route;
6. `serve({ fetch: api.fetch, port, hostname: host })`;
7. writes `daemon.json` best-effort;
8. returns `{ port, instanceId }`.

`server.ts` prints
`ssh-tunnel daemon on http://<host>:<port> (<instanceId>)`.

**Section sources**

- [packages/ssh-service/src/serve.ts](file://packages/ssh-service/src/serve.ts#L40-L105)
- [apps/server/src/server.ts](file://apps/server/src/server.ts#L40-L75)

## Static Web UI

When `--web-dir` is set, `GET /*` serves:

- `/` → `index.html`;
- any existing file under the directory with a MIME type from a small map
  (`.html`, `.js`, `.css`, `.json`, `.svg`, `.png`, `.ico`);
- a missing file → `index.html` (SPA fallback), or `404 "web UI not built"` if
  there is no index;
- any path containing `..` → `404`.

Everything else is `application/octet-stream`. There is no compression, cache
header, ETag or directory listing; each request reads the file synchronously.

**Section sources**

- [packages/ssh-service/src/serve.ts](file://packages/ssh-service/src/serve.ts#L60-L90)

## Discovery File

`daemon.json` is written next to the profile store:

```json
{ "port": 7432, "host": "127.0.0.1", "instanceId": "svc-…", "version": "0.1.0", "pid": 12345 }
```

The CLI and TUI read `host` and `port` from it. It is written once at startup
and never removed. A crashed daemon therefore leaves a stale file pointing at a
port nobody is listening on; clients use it as-is and fail to connect until the
daemon is restarted or `--daemon=` is passed explicitly.

**Section sources**

- [packages/ssh-service/src/serve.ts](file://packages/ssh-service/src/serve.ts#L100-L108)
- [apps/cli/src/cli.ts](file://apps/cli/src/cli.ts#L22-L36)

## Operational Behaviour

- **Loopback only by default.** Binding `0.0.0.0` is possible with `--host`
  but there is no authentication, so that exposes tunnel control to the
  network. CORS is open for `/api/*`; the security assumption is loopback.
- **Tunnels survive the daemon.** `ssh` children are spawned detached with
  `unref()`; quitting the daemon does not kill them. On restart, `runtime.json`
  + control sockets reclaim them.
- **No signal handling.** There is no `SIGTERM`/`SIGINT` hook, no graceful
  shutdown, and no cleanup of `daemon.json`. The process exits when the event
  loop empties or the signal default kills it.
- **Logging is stdout only.** One startup line; request logging is absent.
- **One manager per process.** Profile/state maps are in memory after first
  load; there is no shared cache between daemon instances. Two daemons on
  different ports writing the same store would race.

**Section sources**

- [packages/ssh-host-node/src/process.ts](file://packages/ssh-host-node/src/process.ts#L10-L40)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L154-L236)
- [README.md](file://README.md#L35-L39)

## Gaps

1. **No auth or token**, even when bound to a non-loopback host.
2. **No graceful shutdown**, so `daemon.json` is never cleaned up.
3. **No request logging or metrics**, which makes field debugging rely on the
   per-tunnel ssh log.
4. **Static file reads are synchronous and uncached**; fine for a local console,
   wasteful on every reload.
5. **No `--version` flag**; the version is only in `daemon.json` and
   `/api/health`.
6. **No port-in-use handling**; `serve()` throws and the process exits with a
   stack trace.

**Section sources**

- [packages/ssh-service/src/serve.ts](file://packages/ssh-service/src/serve.ts#L88-L108)
- [apps/server/src/server.ts](file://apps/server/src/server.ts#L1-L30)
