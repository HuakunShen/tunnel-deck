# TUI

<cite>
**Referenced Files in This Document**
- [apps/tui/package.json](file://apps/tui/package.json)
- [apps/tui/src/tui.tsx](file://apps/tui/src/tui.tsx)
- [apps/tui/src/view.tsx](file://apps/tui/src/view.tsx)
- [apps/tui/src/form.tsx](file://apps/tui/src/form.tsx)
- [apps/tui/src/view.test.tsx](file://apps/tui/src/view.test.tsx)
- [apps/tui/scripts/build.ts](file://apps/tui/scripts/build.ts)
- [apps/tui/scripts/shot.ts](file://apps/tui/scripts/shot.ts)
- [tests/e2e/tui.test.ts](file://tests/e2e/tui.test.ts)
- [tests/contract/tui-bundle.test.ts](file://tests/contract/tui-bundle.test.ts)
- [packages/build-helper/src/index.ts](file://packages/build-helper/src/index.ts)
</cite>

## Table of Contents

1. [Shape](#shape)
2. [App State and Keys](#app-state-and-keys)
3. [List View](#list-view)
4. [Creation Form](#creation-form)
5. [Daemon and Direct Modes](#daemon-and-direct-modes)
6. [Building](#building)
7. [Tests](#tests)
8. [Gaps](#gaps)

## Shape

`apps/tui` is a fullscreen terminal front over the same API as the Web console.
It uses `@opentui/core` for the renderer and `@opentui/react` for components,
with React 19. It depends on `ssh-client`, `ssh-contract`, `ssh-core`
(unused at runtime but available), `ssh-host-node` and `ssh-service` for
`--direct`.

There are three source modules:

- `tui.tsx` — the interactive `App`, keyboard handling, polling;
- `view.tsx` — a pure `TunnelListView` (no hooks, timers or I/O);
- `form.tsx` — a pure creation form (props in, validated input out).

**Section sources**

- [apps/tui/package.json](file://apps/tui/package.json#L1-L35)
- [apps/tui/src/tui.tsx](file://apps/tui/src/tui.tsx#L1-L60)

## App State and Keys

`App` keeps one snapshot `{ profiles, states, error }`, the selected index, a
`busy` flag, a `notice` line, and `formOpen`. It refreshes on mount and every
2500 ms, and reloads after every mutation.

| Key | Action |
|---|---|
| `q` / `Ctrl-C` | quit (works even while the form is open) |
| `↑` / `k` | move selection up (wraps) |
| `↓` / `j` | move selection down (wraps) |
| `Enter` | start when stopped, stop when running/degraded |
| `n` | open the creation form |
| `d` | delete the selected profile |
| `p` | probe the selected profile |
| `s` | stop all |
| `r` | refresh |

While the form is open it owns `↑`/`↓`/`Tab`/`Enter`/`Esc`; global keys except
`q` are suspended. `range` keys (`Home`/`End`, `g`/`G`) are not implemented.

Errors are rendered as `CODE: message` when they are `TunnelError`, else
"cannot reach backend".

**Section sources**

- [apps/tui/src/tui.tsx](file://apps/tui/src/tui.tsx#L58-L200)

## List View

`TunnelListView` renders:

- a title line `ssh-tunnel — one-click tunnel manager` (plus `(working…)` when
  busy);
- an error line when backend access failed;
- a bordered 16-row list: selection marker, status dot, name, `[mode]`, target;
- a bordered 10-row detail pane: name/mode, target with jump hops, one line per
  forward (`-L`, `-R`, `-D` with bind address and target), pid, last error, and
  probe results;
- a notice line with the key hints.

Selection is a full-row highlight. The background is applied to the row `box`,
not the `text`, because OpenTUI paints text backgrounds per glyph run and a
text-level background stops at the last character. That was a real rendering
bug fixed by moving the style to the wrapper; `scripts/verify-highlight.ts`
exists to check it.

**Section sources**

- [apps/tui/src/view.tsx](file://apps/tui/src/view.tsx#L1-L102)

## Creation Form

`TunnelForm` is six controlled fields: name, mode, target, local port, remote
host, remote port. `↑`/`↓`/`Tab` move focus, `Esc` cancels, `Enter` submits.

`validateForm` requires a name, a target, a local port 1..65535, and (unless
mode is `dynamic`) a remote host and remote port 1..65535.

`toProfileInput` builds exactly one forward:

- `dynamic` → `{kind:"dynamic", bindPort}`;
- `remote` → `{kind:"remote", bindPort, targetHost, targetPort}`;
- anything else → `{kind:"local", …}`.

The mode field accepts any string found in `TUNNEL_MODES`, so typing `jump` or
`multi` stores that mode but still emits a local forward. The label says
"local/remote/dynamic" and the hint says the mode cycles through those three,
which is the intended set.

**Section sources**

- [apps/tui/src/form.tsx](file://apps/tui/src/form.tsx#L1-L167)
- [packages/ssh-contract/src/tunnel.ts](file://packages/ssh-contract/src/tunnel.ts#L60-L70)

## Daemon and Direct Modes

- Default: `createTunnelClient({ baseUrl: daemonBase(), fetch })`, with the same
  `--daemon=` / `daemon.json` / `127.0.0.1:7432` resolution as the CLI.
- `--direct`: constructs a `TunnelManager` with process, probe, store, clock and
  logDir only. No terminal port, no control port, no runtime store — so a
  `--direct` TUI cannot adopt daemon-started tunnels and cannot launch a
  terminal.

`main()` creates the renderer with `createCliRenderer()` and renders `App`.
There is no alternate screen management in this file; OpenTUI owns the
terminal.

**Section sources**

- [apps/tui/src/tui.tsx](file://apps/tui/src/tui.tsx#L190-L211)

## Building

`bun ./scripts/build.ts` bundles `src/tui.tsx` to `dist/tui.mjs` with:

- `jsx: "automatic"`;
- `external: TUI_EXTERNALS` (react + every OpenTUI platform binding);
- `PRODUCTION_DEFINE` (`process.env.NODE_ENV = "production"`).

The bundle intentionally requires `apps/tui/node_modules` at runtime. Bundling
React a second time caused `Cannot read properties of null (reading
'useState')`; `tests/contract/tui-bundle.test.ts` exists to prevent a relapse.

`scripts/shot.ts` renders SVG screenshots from memory frames, and the README's
TUI gallery image is generated that way.

**Section sources**

- [apps/tui/scripts/build.ts](file://apps/tui/scripts/build.ts#L1-L25)
- [packages/build-helper/src/index.ts](file://packages/build-helper/src/index.ts#L62-L78)
- [tests/contract/tui-bundle.test.ts](file://tests/contract/tui-bundle.test.ts#L1-L40)

## Tests

| Layer | File | Runs where |
|---|---|---|
| pure view snapshots | `apps/tui/src/view.test.tsx` | `pnpm test` (OpenTUI test-renderer, no TTY) |
| pure form | `apps/tui/src/form.test.tsx` | `pnpm test` |
| bundle shape | `tests/contract/tui-bundle.test.ts` | after `pnpm build` |
| real terminal e2e | `tests/e2e/tui.test.ts` | `pnpm test:e2e`, self-skips without a pty or bundle |

The README records that four render-snapshot assertions fail under the vitest
setup because OpenTUI's native renderer is unavailable, which is why CI runs
`pnpm test:ci` (excluding `apps/tui/**`) rather than `pnpm test`. The e2e suite
uses `@microsoft/tui-test`, boots `dist/tui.mjs --daemon=…`, and captures SVG
screenshots.

**Section sources**

- [README.md](file://README.md#L146-L151)
- [tests/e2e/tui.test.ts](file://tests/e2e/tui.test.ts#L1-L60)
- [.github/workflows/ci.yml](file://.github/workflows/ci.yml#L30-L45)

## Gaps

1. **Delete has no confirmation.** `d` immediately deletes the selected profile
   (the backend refuses if it is running, but a stopped profile is gone).
2. **No edit flow.** Profiles can be created here but only edited in the Web UI
   or by rewriting JSON.
3. **No log view, no terminal launch, no host inventory.** Those exist in the
   Web console/CLI but not in the TUI.
4. **The form exposes one forward only**, so `multi` profiles cannot be created
   interactively.
5. **`jump`/`multi` modes can be typed but produce a local forward**, as above.
6. **`ssh-core` is a dependency but not imported** by runtime code; the TUI does
   not reuse validation from core.

**Section sources**

- [apps/tui/src/tui.tsx](file://apps/tui/src/tui.tsx#L140-L180)
- [apps/tui/src/form.tsx](file://apps/tui/src/form.tsx#L50-L90)
- [apps/tui/package.json](file://apps/tui/package.json#L15-L25)
