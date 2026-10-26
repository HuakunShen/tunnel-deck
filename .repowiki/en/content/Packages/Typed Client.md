# Typed Client

<cite>
**Referenced Files in This Document**
- [packages/ssh-client/package.json](file://packages/ssh-client/package.json)
- [packages/ssh-client/src/index.ts](file://packages/ssh-client/src/index.ts)
- [packages/ssh-contract/src/errors.ts](file://packages/ssh-contract/src/errors.ts)
- [packages/ssh-contract/src/routes.ts](file://packages/ssh-contract/src/routes.ts)
- [apps/web/src/lib/store.ts](file://apps/web/src/lib/store.ts)
- [apps/desktop/src/index.ts](file://apps/desktop/src/index.ts)
- [apps/desktop/src-tauri/src/tunnel_commands.rs](file://apps/desktop/src-tauri/src/tunnel_commands.rs)
</cite>

## Table of Contents

1. [Purpose](#purpose)
2. [HTTP Client](#http-client)
3. [Invoke Client](#invoke-client)
4. [Command Map](#command-map)
5. [Validation and Errors](#validation-and-errors)
6. [Consumer Notes](#consumer-notes)
7. [Gaps](#gaps)

## Purpose

`@ssh-tunnel/client` gives every front the same typed surface regardless of
transport. It imports only `@ssh-tunnel/contract` and never names Tauri, Node or
the DOM: the caller injects `fetch`, or an `invoke` function. That is why the
same client runs in a browser, a webview, a Worker, the TUI and tests.

**Section sources**

- [packages/ssh-client/src/index.ts](file://packages/ssh-client/src/index.ts#L1-L10)
- [packages/ssh-client/package.json](file://packages/ssh-client/package.json#L1-L20)

## HTTP Client

```ts
createTunnelClient({ baseUrl, fetch }) // -> TunnelClient
```

Every method funnels through `request(options, method, path, parse, body?)`:

- URL is `${baseUrl}${path}`; the caller decides whether `baseUrl` is `""`
  (same origin), a loopback URL, or an override.
- `content-type: application/json` is set only when a body exists.
- A thrown `fetch` becomes
  `TunnelError("InternalError", "cannot reach daemon at …", { retryable: true })`.
- A non-OK response reads `payload.problem`; an unrecognized shape becomes
  `InternalError`.
- A successful body is parsed by the caller's schema; a parse failure becomes
  `InternalError("daemon response failed validation")`.

`must(parsed)` unwraps zod's `{ success, data }`, turning `success: false` into
the same validation error.

**Section sources**

- [packages/ssh-client/src/index.ts](file://packages/ssh-client/src/index.ts#L34-L112)

## Invoke Client

```ts
createInvokeTunnelClient(invoke) // -> TunnelClient
```

`InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>`.
The implementation calls `invoke(cmd, args)`, maps a rejection through
`problemToError` (so a Tauri `CommandProblem` becomes a `TunnelError` with the
same code), then validates the resolved body with the same schemas and `.loose()`.

This is how the desktop app avoids a second API: the Web console's components
never learn which transport they are on.

**Section sources**

- [packages/ssh-client/src/index.ts](file://packages/ssh-client/src/index.ts#L150-L316)

## Command Map

| Client method | HTTP | Tauri command |
|---|---|---|
| `health` | `GET /api/health` | `tunnel_health` |
| `listProfiles` | `GET /api/profiles` | `tunnel_list_profiles` |
| `createProfile` | `POST /api/profiles` | `tunnel_create_profile` |
| `getProfile` | `GET /api/profiles/:id` | `tunnel_get_profile` |
| `updateProfile` | `PUT /api/profiles/:id` | `tunnel_update_profile` |
| `deleteProfile` | `DELETE /api/profiles/:id` | `tunnel_delete_profile` |
| `preview` | `GET /api/profiles/:id/preview` | `tunnel_preview` |
| `listStates` | `GET /api/states` | `tunnel_list_states` |
| `getState` | `GET /api/states/:id` | `tunnel_get_state` |
| `start` | `POST /api/states/:id/start` | `tunnel_start` |
| `stop` | `POST /api/states/:id/stop` | `tunnel_stop` |
| `stopAll` | `POST /api/states/stop-all` | `tunnel_stop_all` |
| `probe` | `POST /api/states/:id/probe` | `tunnel_probe` |
| `listHosts` | `GET /api/ssh-hosts` | `tunnel_list_hosts` |
| `listTerminals` | `GET /api/terminals` | `tunnel_list_terminals` |
| `getLog` | `GET /api/profiles/:id/log?tail=` | `tunnel_log_tail` |
| `launchInTerminal` | `POST /api/profiles/:id/launch` | `tunnel_launch_in_terminal` |
| `openUrl` | `POST /api/open-url` | `tunnel_open_url` |
| `closeSession` | `DELETE /api/profiles/:id/session` | `tunnel_close_session` |

**Section sources**

- [packages/ssh-client/src/index.ts](file://packages/ssh-client/src/index.ts#L115-L316)
- [apps/desktop/src-tauri/src/tunnel_commands.rs](file://apps/desktop/src-tauri/src/tunnel_commands.rs#L218-L387)

## Validation and Errors

- Every list response is unwrapped from its envelope
  (`{profiles}`, `{states}`, `{hosts}`, `{terminals}`), so callers get arrays.
- `deleteProfile` and `closeSession` resolve `undefined`; they do not parse a
  body. `closeSession` therefore cannot observe the HTTP `404` that the daemon
  returns when no session exists — the request helper throws before the
  no-op parser runs only when the response is non-OK, so a 404 *does* throw
  and a Tauri `Ok(false)` does not.
- `preview` returns the daemon's `{ profileId, argv, command }` unchanged.
- `getLog` passes `tail` through as a query string only when defined; the
  server clamps it.

**Section sources**

- [packages/ssh-client/src/index.ts](file://packages/ssh-client/src/index.ts#L160-L300)

## Consumer Notes

- **Web**: `getClient()` caches one instance per page; `refreshAll` calls
  `listProfiles` and `listStates` concurrently.
- **Desktop**: `apps/desktop/src/index.ts` exposes
  `createDesktopClient({ invoke })`, a one-line wrapper around
  `createInvokeTunnelClient`.
- **VS Code**: builds one client around the resolved daemon URL and calls it
  from commands and the tree provider.
- **DSH**: does not use the client at all; it mounts the Hono app in-process and
  serves the Web UI, which uses the client in the browser.

**Section sources**

- [apps/web/src/lib/store.ts](file://apps/web/src/lib/store.ts#L60-L130)
- [apps/desktop/src/index.ts](file://apps/desktop/src/index.ts#L1-L24)
- [integrations/vscode/src/extension.ts](file://integrations/vscode/src/extension.ts#L28-L50)

## Gaps

1. **No abort/timeout support.** A hung request relies on the daemon's own
   deadlines (control 2 s, probe 2.5 s × 3) and `fetch` defaults.
2. **No retry policy.** `retryable: true` is reported but nothing retries; the
   fronts retry by polling.
3. **Literal paths.** Several methods use literals instead of the route table,
   and `closeSession` has no helper at all.
4. **`TunnelClient` is a structural `ReturnType`**, not a hand-written
   interface, so it cannot be implemented by another class without matching the
   whole object.
5. **Body typing is loose at the invoke boundary.** `args` is
   `Record<string, unknown>`; correctness depends on the Rust command
   signatures and serde field names.

**Section sources**

- [packages/ssh-client/src/index.ts](file://packages/ssh-client/src/index.ts#L112-L160)
- [packages/ssh-contract/src/routes.ts](file://packages/ssh-contract/src/routes.ts#L40-L70)
