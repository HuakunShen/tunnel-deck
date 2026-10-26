# VS Code Extension

<cite>
**Referenced Files in This Document**
- [integrations/vscode/package.json](file://integrations/vscode/package.json)
- [integrations/vscode/src/extension.ts](file://integrations/vscode/src/extension.ts)
- [integrations/vscode/src/supervisor.ts](file://integrations/vscode/src/supervisor.ts)
- [integrations/vscode/src/tree.ts](file://integrations/vscode/src/tree.ts)
- [integrations/vscode/scripts/build.ts](file://integrations/vscode/scripts/build.ts)
- [tests/contract/vscode.test.ts](file://tests/contract/vscode.test.ts)
- [apps/server/src/server.ts](file://apps/server/src/server.ts)
</cite>

## Table of Contents

1. [Shape](#shape)
2. [Daemon Supervisor](#daemon-supervisor)
3. [Tree View](#tree-view)
4. [Commands and Webview](#commands-and-webview)
5. [Build and Packaging](#build-and-packaging)
6. [Tests and Gaps](#tests-and-gaps)

## Shape

`integrations/vscode` is a real extension in the space-lens shape:
`supervisor.ts` attaches to a healthy daemon or spawns a staged one; `tree.ts`
renders the sidebar; `extension.ts` wires commands and a webview. It depends only
on `@ssh-tunnel/client` and `@ssh-tunnel/contract`; all tunnel rules stay in the
daemon.

`package.json` contributes:

- an activity-bar container `sshTunnel` with a "Tunnels" view
  (`sshTunnel.tunnels`);
- commands `ssh-tunnel.start`, `.stop`, `.probe`, `.stopAll`, `.refresh`,
  `.openWorkbench`;
- view-title and item-context menus (start only on `tunnelStopped`, stop only on
  `tunnelRunning`);
- setting `sshTunnel.daemonUrl` (empty = `http://127.0.0.1:7432`).

**Section sources**

- [integrations/vscode/package.json](file://integrations/vscode/package.json#L1-L131)

## Daemon Supervisor

`ensureDaemon({ daemonUrl, serverDir, storePath?, timeoutMs?, fetch? })`:

1. `GET <url>/api/health`; if OK, return
   `{ readiness: {url, serviceInstanceId}, owned: false, stop(){} }` — an
   attached daemon is never killed.
2. Otherwise look for `<serverDir>/server.mjs` and spawn
   `node server.mjs --port=7432 --host=127.0.0.1`; if absent, spawn a native
   `ssh-tunnel-serve` beside the extension (adding `--store` when provided).
3. Swallow the child's `error` event so a missing binary does not become an
   uncaught exception; `waitFor` still times out.
4. Poll health every 250 ms up to 15 s (default). On timeout, SIGTERM the child
   and throw "ssh-tunnel daemon did not become ready; is node installed and port
   7432 free?".
5. Return `owned: true` with a `stop()` that SIGTERMs the child.

The `fetch` injectable is what makes the attach/spawn decisions testable without
an extension host.

**Section sources**

- [integrations/vscode/src/supervisor.ts](file://integrations/vscode/src/supervisor.ts#L1-L119)

## Tree View

`TunnelTreeProvider`:

- takes a `TunnelClient`, a `VscodeApi` object (the subset of `vscode` it needs),
  a base URL and `fetch`;
- fetches profiles + states, pairs them, and builds `TunnelRow { profile, status,
  detail }`;
- `detail` is a one-line forward summary (`-L 8080→host:80`, `-D 1080`, …);
- sets `contextValue` to `tunnelRunning`/`tunnelStopped` for menu `when` clauses;
- uses `ThemeIcon` play/stop and fires its own change event;
- `refresh()` is called on demand and on a 3 s poll from `extension.ts`.

Keeping the API surface as an interface means the provider is tested without
importing `vscode`.

**Section sources**

- [integrations/vscode/src/tree.ts](file://integrations/vscode/src/tree.ts#L1-L107)

## Commands and Webview

`activate(context)`:

1. creates an output channel;
2. resolves `sshTunnel.daemonUrl`, calls `ensureDaemon`, logs readiness or shows
   an error;
3. registers the owned daemon's `stop()` as a disposable;
4. creates the client against the resolved URL;
5. registers the tree provider and a 3 s refresh interval;
6. registers start/stop/probe/stop-all/refresh/openWorkbench.

Start/stop/probe accept a tree row or fall back to a `showQuickPick` of all rows.
Success/failure is written to the output channel and/or shown as an error
message, then the tree refreshes.

`openWorkbench` creates a webview panel with scripts enabled and an `<iframe>`
pointing at the daemon's `/`. It is a frame of the real console, not a
reimplementation.

`deactivate()` is empty because the disposables (owned daemon, interval)
registered in `context.subscriptions` do the cleanup.

**Section sources**

- [integrations/vscode/src/extension.ts](file://integrations/vscode/src/extension.ts#L1-L125)

## Build and Packaging

`scripts/build.ts`:

- esbuild `src/extension.ts` → `dist/extension.js`, CJS, `platform: node`,
  `target: node20`, `external: ["vscode"]`;
- copies `apps/server/dist/server.mjs` beside it so the supervisor can spawn a
  daemon; a missing server prints a non-fatal warning.

`pnpm package` runs the build then `npx @vscode/vsce package --no-dependencies`,
producing `ssh-tunnel-0.1.0.vsix`. Turbo orders this build after the server
build.

**Section sources**

- [integrations/vscode/scripts/build.ts](file://integrations/vscode/scripts/build.ts#L1-L45)
- [integrations/vscode/package.json](file://integrations/vscode/package.json#L110-L131)
- [turbo.json](file://turbo.json#L25-L40)

## Tests and Gaps

`tests/contract/vscode.test.ts` uses vscode-free doubles to cover:

- attach to a healthy daemon (no spawn);
- spawn failure and readiness timeout;
- tree rendering and context values;
- offline/clear behaviour.

Gaps:

1. **The extension surfaces only start/stop/probe/stop-all/refresh/open**; it
   cannot create, edit or delete a profile from the sidebar — the workbench frame
   is required.
2. **No log view**; a failure message must be read from the output channel or the
   Web console.
3. **Fixed port 7432** when spawning; a busy port is reported as "did not become
   ready", not as a port conflict.
4. **`stop()` uses SIGTERM only** and does not wait for exit.
5. **`as unknown as` casts** at `extension.ts:46` and `:48` are required by the
   vscode-free test design but violate the `AGENTS.md` convention.

**Section sources**

- [tests/contract/vscode.test.ts](file://tests/contract/vscode.test.ts#L1-L104)
- [integrations/vscode/src/extension.ts](file://integrations/vscode/src/extension.ts#L40-L55)
- [integrations/vscode/src/supervisor.ts](file://integrations/vscode/src/supervisor.ts#L85-L119)
