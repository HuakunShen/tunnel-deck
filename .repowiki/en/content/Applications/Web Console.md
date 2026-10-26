# Web Console

<cite>
**Referenced Files in This Document**
- [apps/web/package.json](file://apps/web/package.json)
- [apps/web/svelte.config.js](file://apps/web/svelte.config.js)
- [apps/web/src/routes/+layout.svelte](file://apps/web/src/routes/+layout.svelte)
- [apps/web/src/routes/+page.svelte](file://apps/web/src/routes/+page.svelte)
- [apps/web/src/lib/store.ts](file://apps/web/src/lib/store.ts)
- [apps/web/src/lib/TunnelRow.svelte](file://apps/web/src/lib/TunnelRow.svelte)
- [apps/web/src/lib/TunnelEditor.svelte](file://apps/web/src/lib/TunnelEditor.svelte)
- [apps/web/src/lib/LogSheet.svelte](file://apps/web/src/lib/LogSheet.svelte)
- [apps/web/src/lib/SettingsSheet.svelte](file://apps/web/src/lib/SettingsSheet.svelte)
- [apps/web/src/app.css](file://apps/web/src/app.css)
- [apps/web/scripts/build.ts](file://apps/web/scripts/build.ts)
- [packages/ssh-core/src/endpoint.ts](file://packages/ssh-core/src/endpoint.ts)
- [tests/contract/shell-geometry.test.ts](file://tests/contract/shell-geometry.test.ts)
</cite>

## Table of Contents

1. [Stack and Output](#stack-and-output)
2. [Data Layer](#data-layer)
3. [Console Shell](#console-shell)
4. [Row Actions](#row-actions)
5. [Editor](#editor)
6. [Log Sheet](#log-sheet)
7. [Appearance](#appearance)
8. [Polling and Feedback](#polling-and-feedback)
9. [Test Hooks](#test-hooks)
10. [Gaps](#gaps)

## Stack and Output

The Web console is SvelteKit 2 + Svelte 5 + Tailwind 4 + Vite, built as a
single static SPA:

- `adapter-static` with `pages: "dist"`, `assets: "dist"`,
  `fallback: "index.html"`, `strict: true`;
- no server routes — a server route fails the build;
- output goes to `apps/web/dist` because the daemon serves that directory
  (`--web-dir`), Tauri embeds it as `frontendDist`, DSH copies it, and VS Code
  frames the daemon.

One page (`src/routes/+page.svelte`), one layout, five local components.

**Section sources**

- [apps/web/svelte.config.js](file://apps/web/svelte.config.js#L1-L25)
- [apps/web/scripts/build.ts](file://apps/web/scripts/build.ts#L1-L20)
- [apps/web/package.json](file://apps/web/package.json#L1-L35)

## Data Layer

`store.ts` owns:

- **transport selection** (`getClient`): Tauri IPC → VS Code loopback → `?daemon=`
  override → same-origin HTTP;
- `initialStore` (`profiles`, `states`, `hosts`, `terminals`, `connected`,
  `lastError`, `loading`);
- `stateOf(store, profileId)`;
- `refreshAll` (profiles + states in parallel; connection flag and error);
- `refreshHosts` and `refreshTerminals` (one-shot reads; failures degrade to
  empty rather than an error);
- `openUrl` (delegates to the daemon so a webview does not navigate away).

Components never construct a client directly; they call `getClient()`, which
caches a single instance per page.

**Section sources**

- [apps/web/src/lib/store.ts](file://apps/web/src/lib/store.ts#L1-L148)

## Console Shell

`+page.svelte` renders the space-lens-style frame:

- **Toolbar**: new tunnel, stop all, refresh, settings, stacked title/subtitle,
  `data-tauri-drag-region="deep"` (macOS Overlay title bar), glass control pill.
- **Sidebar**: filters All / Live / Idle with counts, the SSH host list from
  `~/.ssh/config`, and detected terminals.
- **Content**: one `TunnelRow` per visible profile, empty state, error state.
- **Status bar**: running/failed counts and the last error.
- **Sheets**: `TunnelEditor`, `LogSheet`, `SettingsSheet`.

The layout sets `data-interface` from the user agent and `data-native-chrome`
when `location.protocol === "tauri:"`; `?theme=dark|light` overrides the OS
preference for screenshots and e2e.

**Section sources**

- [apps/web/src/routes/+page.svelte](file://apps/web/src/routes/+page.svelte#L270-L360)
- [apps/web/src/routes/+layout.svelte](file://apps/web/src/routes/+layout.svelte#L1-L50)

## Row Actions

`TunnelRow.svelte` renders:

- status dot (`bg-signal-running/degraded/failed/starting/idle`, pulsing while
  starting/stopping);
- name, human mode label (Local/Remote/Dynamic/Jump/Multi), status, and an
  origin badge (`terminal` or `external`) when live;
- target plus jump hops;
- one endpoint per forward: only `ssh -L` becomes a button (via
  `localForwardEndpoint`); `-R` and `-D` stay plain text;
- inline error text with a `log` link when `lastError` is set;
- actions: Start/Stop (spinner while busy), copy command, copy endpoint,
  "Open in ▾" menu, log, edit, delete.

The "Open in" menu lists terminals from `listTerminals()` and offers two modes
per terminal: the tunnel command (`ssh -N`) or an interactive **shell**
(`shell` drops `-N`). The daemon spawns the terminal, so the window can be
closed from the UI.

**Section sources**

- [apps/web/src/lib/TunnelRow.svelte](file://apps/web/src/lib/TunnelRow.svelte#L85-L323)
- [packages/ssh-core/src/endpoint.ts](file://packages/ssh-core/src/endpoint.ts#L20-L60)

## Editor

`TunnelEditor.svelte` is a create/edit dialog covering:

- name, mode (`local`, `remote`, `dynamic`, `jump`, `multi`) with a one-line
  explanation per mode;
- target, jump hops (comma-separated), user, SSH port, identity file, notes;
- a list of forwards with per-row kind cards, bind address/port, target
  host/port, add/remove.

It validates ports and required target fields locally, then emits a
`TunnelProfileInput`. It does **not** expose `extraOptions`,
`serverAliveInterval`, `exitOnForwardFailure`, `clearAllForwardings`,
`noRemoteCommand`, `remoteCommand`, or `autoReconnect`, although the contract
supports them. Those fields can still be set through the CLI (`add --file`) or
a hand-edited profile JSON.

**Section sources**

- [apps/web/src/lib/TunnelEditor.svelte](file://apps/web/src/lib/TunnelEditor.svelte#L1-L110)
- [packages/ssh-contract/src/tunnel.ts](file://packages/ssh-contract/src/tunnel.ts#L80-L115)

## Log Sheet

`LogSheet.svelte` reads the tail on open (`getLog(id, 500)`), scrolls to the
bottom after render, shows the absolute path and byte count, supports refresh
and copy, and surfaces a read error inline. This is the answer to failures like
`ssh exited with code 255; see /path`: the path is one click from the message.

**Section sources**

- [apps/web/src/lib/LogSheet.svelte](file://apps/web/src/lib/LogSheet.svelte#L1-L108)

## Appearance

- `SettingsSheet` exposes one knob: window transparency, `0.4`–`0.94`, default
  `0.72`, stored in `localStorage` under `ssh-tunnel.material-opacity`.
- The value is written to the CSS custom property `--material-opacity` and only
  matters when the desktop host published `glass`/`vibrancy`; in a browser the
  setting is shown as unavailable/inert.
- `applyOpacity` tolerates storage failures (private browsing) and keeps the
  default.
- Dark/light follows `prefers-color-scheme` unless `?theme=` overrides it.

**Section sources**

- [apps/web/src/routes/+page.svelte](file://apps/web/src/routes/+page.svelte#L30-L80)
- [apps/web/src/lib/SettingsSheet.svelte](file://apps/web/src/lib/SettingsSheet.svelte#L1-L79)

## Polling and Feedback

- One interval polls `refreshAll` every 3000 ms; hosts and terminals are
  re-read only when the connection transitions from down to up, so a later
  daemon start repopulates the sidebar without a manual refresh.
- `act(id, label, fn, optimistic?)` guards against double submits with a
  `busyIds` set, patches the row to an optimistic status (`starting` on start,
  `stopping` on stop) so the UI does not look frozen during the backend's grace
  windows, then refreshes.
- Toasts last 2600 ms.
- Errors land in `store.lastError` and/or the row's `lastError` line.

This optimistic patch is the newest change in the repository (`0675e0e`).

**Section sources**

- [apps/web/src/routes/+page.svelte](file://apps/web/src/routes/+page.svelte#L104-L162)
- [apps/web/src/routes/+page.svelte](file://apps/web/src/routes/+page.svelte#L249-L260)

## Test Hooks

- `?editor=open` auto-opens the creation dialog for screenshots and e2e; it is
  a query parameter, not test-only UI.
- Desktop e2e relies on stable class names (`.tunnel-toolbar`,
  `.tunnel-sidebar`, `.tunnel-content`, `.tunnel-row`) and on the
  `--material-opacity` custom property.
- `tests/contract/shell-geometry.test.ts` pins the shell dimensions from
  `packages/web-ui/src/shell.css`, which this page consumes.

**Section sources**

- [apps/web/src/routes/+page.svelte](file://apps/web/src/routes/+page.svelte#L249-L270)
- [apps/desktop/e2e/specs/app.js](file://apps/desktop/e2e/specs/app.js#L1-L60)
- [tests/contract/shell-geometry.test.ts](file://tests/contract/shell-geometry.test.ts#L1-L30)

## Gaps

1. **Stale comment about a sidecar.** The refresh comment still says "the
   desktop shell loads the page before the sidecar answers"; the desktop app
   has no sidecar and does not use HTTP.
2. **Editor is a subset of the contract** (see [Editor](#editor)).
3. **`confirm()` for delete** is a blocking native dialog and cannot be styled
   or tested through the DOM.
4. **Polling only.** There is no SSE/WebSocket push; state changes are observed
   up to 3 s late.
5. **No pagination or virtualization.** Every profile renders; the cap is 200 on
   the backend.
6. **`window as unknown as`** at line 70 violates the `AGENTS.md` convention.
7. **Appearance is desktop-only** but the setting is stored in every host's
   localStorage; the Web/DSH/VS Code builds simply never see a material.

**Section sources**

- [apps/web/src/routes/+page.svelte](file://apps/web/src/routes/+page.svelte#L110-L120)
- [apps/web/src/routes/+page.svelte](file://apps/web/src/routes/+page.svelte#L196-L200)
- [apps/web/src/routes/+page.svelte](file://apps/web/src/routes/+page.svelte#L68-L74)
