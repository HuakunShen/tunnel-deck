# Boundaries and Drift

<cite>
**Referenced Files in This Document**
- [AGENTS.md](file://AGENTS.md)
- [README.md](file://README.md)
- [scripts/check-boundaries.ts](file://scripts/check-boundaries.ts)
- [pnpm-workspace.yaml](file://pnpm-workspace.yaml)
- [integrations/dsh/package.json](file://integrations/dsh/package.json)
- [integrations/dsh/src/host.ts](file://integrations/dsh/src/host.ts)
- [integrations/vscode/src/extension.ts](file://integrations/vscode/src/extension.ts)
- [apps/web/src/routes/+page.svelte](file://apps/web/src/routes/+page.svelte)
- [apps/desktop/src/index.ts](file://apps/desktop/src/index.ts)
- [apps/desktop/package.json](file://apps/desktop/package.json)
- [apps/desktop/src-tauri/Cargo.toml](file://apps/desktop/src-tauri/Cargo.toml)
- [packages/ssh-contract/src/routes.ts](file://packages/ssh-contract/src/routes.ts)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs)
- [docs/design.md](file://docs/design.md)
</cite>

## Table of Contents

1. [What Is Machine-Checked](#what-is-machine-checked)
2. [What Is Not Checked](#what-is-not-checked)
3. [AGENTS.md vs the Code](#agentsmd-vs-the-code)
4. [README vs the Code](#readme-vs-the-code)
5. [Stale Comments Inside Working Code](#stale-comments-inside-working-code)
6. [Rust/TypeScript Behavioural Drift](#rusttypescript-behavioural-drift)
7. [Contract Gaps](#contract-gaps)
8. [Recommended Follow-Ups](#recommended-follow-ups)

## What Is Machine-Checked

`scripts/check-boundaries.ts` is the only automated architectural guard, run as
`pnpm check:boundaries` (which invokes it with Bun). It scans only three
packages — `packages/ssh-contract`, `packages/ssh-core`,
`packages/ssh-client` — and fails on these patterns:

| Pattern | Message |
|---|---|
| `from "node:..."` / `require("node:...")` | portable package imports node:* |
| `process.env`, `process.exit`, `process.kill`, `process.pid` | portable package touches process |
| `document.`, `window.`, `localStorage` | portable package touches DOM at runtime |
| `@ssh-tunnel/host-node` | portable package imports host layer |
| `@ssh-tunnel/service` | portable package imports service layer |
| `from "../..."` | portable package escapes its own `src` |

It reports `file:line:column` and exits non-zero. CI runs it after `pnpm check`.

**Section sources**

- [scripts/check-boundaries.ts](file://scripts/check-boundaries.ts#L1-L72)
- [.github/workflows/ci.yml](file://.github/workflows/ci.yml#L30-L42)

## What Is Not Checked

- **No Rust boundary check.** `crates/tunnel-core` is host-free by construction
  (no tokio, HTTP, Tauri, or WebView dependency), but nothing verifies that a
  future change keeps it so.
- **No check for `apps/*`, `integrations/*`, `packages/web-ui`,
  `packages/build-helper`.** The `AGENTS.md` table has rows for all of them;
  the script covers three.
- **No check for the three-Cargo-workspace isolation.** Root `Cargo.toml`'s
  `members = ["crates/*"]` and the per-shell `[workspace]` declarations are the
  enforcement; no script asserts that `apps/desktop/src-tauri` stays out of the
  root graph.
- **No dependency-direction check.** `integrations/dsh` importing the service
  and host layers is invisible to `check:boundaries`, because integrations are
  not in `PORTABLE`.
- **No fixture gate for persistence/adoption/probing.** Only argv and state
  transitions are adjudicated across languages.

**Section sources**

- [scripts/check-boundaries.ts](file://scripts/check-boundaries.ts#L12-L20)
- [Cargo.toml](file://Cargo.toml#L1-L12)
- [apps/desktop/src-tauri/Cargo.toml](file://apps/desktop/src-tauri/Cargo.toml#L1-L12)

## AGENTS.md vs the Code

`AGENTS.md` is the older policy document. It describes a TypeScript-only
monorepo and predates the Rust core, the GPUI experiment, and the UI kit.

| AGENTS.md claim | Reality in the tree |
|---|---|
| `integrations/*` "May import `ssh-contract`, `ssh-client`" | `integrations/dsh` depends on `@ssh-tunnel/service` and `@ssh-tunnel/host-node` and imports `TunnelManager`, `createServiceApp`, `createFileStorePort`, `createNodeProcessPort`, `createNodeProbePort`, `systemClock` |
| Table covers packages, apps, integrations | Omits `crates/tunnel-core`, `packages/web-ui`, `packages/build-helper`, `apps/gpui`, `fixtures/`, and `scripts/` other than the boundary checker |
| `packages/ssh-service` may import "all packages" | It imports contract, core, host-node, hono and `@hono/node-server`; it does not import `ssh-client` or `web-ui` |
| "Zero `any` / `as unknown as` / `@ts-ignore` in handwritten code" | Shipped code contains `as unknown as` at `apps/web/src/routes/+page.svelte:70`, `integrations/vscode/src/extension.ts:46` and `:48`, and `packages/web-ui/src/components/ui/badge/badge.svelte:44`; tests use it extensively to fake Node streams |
| "Sharp boundaries are enforced ... at `file:line:column`" | True for the three portable TS packages only; no equivalent for Rust or frontends |
| Conventions describe `noEmit` / esbuild / strict TS | Still true (`tsconfig.base.json`, per-package `check`, build-helper) |
| Error model: closed codes, one status per code | Still true in both languages |
| Commit format `type(scope): lowercase complete english sentence` | Followed by the 49 commits in history |

The table's `apps/*` row ("packages only ... no domain logic") is broadly
honoured: `apps/server` parses argv and calls `startDaemon`; `apps/cli` and
`apps/tui` orchestrate client calls and form state. `apps/desktop` contains no
tunnel rules, only command adapters.

**Section sources**

- [AGENTS.md](file://AGENTS.md#L5-L29)
- [integrations/dsh/package.json](file://integrations/dsh/package.json#L20-L35)
- [integrations/dsh/src/host.ts](file://integrations/dsh/src/host.ts#L30-L60)
- [apps/web/src/routes/+page.svelte](file://apps/web/src/routes/+page.svelte#L68-L74)
- [integrations/vscode/src/extension.ts](file://integrations/vscode/src/extension.ts#L44-L50)
- [packages/web-ui/src/components/ui/badge/badge.svelte](file://packages/web-ui/src/components/ui/badge/badge.svelte#L40-L48)

## README vs the Code

The README is partly a product page written before the Rust port and before the
integrations became real.

| README statement | Current code |
|---|---|
| `apps/desktop` — "Tauri shell stub wrapping the Web UI (thin front)" | A real Tauri app; `lib.rs` builds the window, registers 19 commands, installs native material, and runs WebDriver e2e |
| `integrations/dsh` — "DeepSeek Harness plugin stub" | Host half mounts a real manager + Hono app; client half registers the panel |
| `integrations/vscode` — "VS Code extension stub" | Real extension with supervisor, tree provider, commands, webview |
| "pnpm check ... (16 workspace members)" | 14 `package.json` workspace members plus `spikes/uniview-tui` (and `apps/gpui` is not a pnpm member) |
| "Why TypeScript" argues the backend is TypeScript | The installed desktop app uses the Rust core in-process; TypeScript remains the backend for the daemon and other fronts |
| Tunnel kinds table lists `socks-via-remote` | `TUNNEL_MODES` and Rust `TunnelMode` contain `local`, `remote`, `dynamic`, `jump`, `multi` only; `socks-via-remote` is a documented preset, not a stored value |
| "Install ... manager linked in-process (Rust, no runtime)" | Accurate; this is the newest section |

The "Status (verified live 2026-10-11)" block and `docs/design.md`'s "Live-test
evidence (2026-10-11)" are point-in-time records. They are not wrong about that
date, but they predate the Rust port, the runtime store, the log endpoint, and
the release workflow.

**Section sources**

- [README.md](file://README.md#L8-L46)
- [README.md](file://README.md#L63-L103)
- [README.md](file://README.md#L126-L175)
- [packages/ssh-contract/src/tunnel.ts](file://packages/ssh-contract/src/tunnel.ts#L60-L80)
- [crates/tunnel-core/src/contract.rs](file://crates/tunnel-core/src/contract.rs#L78-L91)

## Stale Comments Inside Working Code

These do not change behaviour, but they mislead a reader:

| Location | Comment | Reality |
|---|---|---|
| `apps/desktop/src/index.ts` | "Tauri shell stub ... Until then, this module documents the wiring" | The module is a small `createDesktopClient` helper; the shell is implemented |
| `apps/desktop/package.json` description | "spawns the daemon (or attaches to one)" | No sidecar, no spawn, no attach; the manager is linked |
| `apps/desktop/src-tauri/Cargo.toml` package description | "spawns the tunnel daemon sidecar" | Sidecar removed in commit `85d04e0` |
| `apps/web/src/routes/+page.svelte` | "The desktop shell loads the page before the sidecar answers" | There is no sidecar; the transport is IPC and the comment now describes the VS Code/HTTP case |
| `docs/design.md` architecture diagram | "desktop/DSH/VSCode (stubs, same API)" | All three are implemented, and desktop does not use the API over HTTP |

**Section sources**

- [apps/desktop/src/index.ts](file://apps/desktop/src/index.ts#L1-L24)
- [apps/desktop/package.json](file://apps/desktop/package.json#L1-L10)
- [apps/desktop/src-tauri/Cargo.toml](file://apps/desktop/src-tauri/Cargo.toml#L8-L14)
- [apps/web/src/routes/+page.svelte](file://apps/web/src/routes/+page.svelte#L110-L120)
- [docs/design.md](file://docs/design.md#L21-L40)

## Rust/TypeScript Behavioural Drift

The domain is mirrored, but not perfectly. Verified differences:

1. **Stale control-socket reclaim.** Rust `reclaim_socket` removes a socket file
   that nothing answers behind, and refuses with `Conflict` when a live master
   answers. The TypeScript manager only `mkdir`s the control directory before
   start/handoff and has no reclaim step. On POSIX, a leftover socket can make a
   new `ssh` warn and run without multiplexing — a state the Rust app now avoids
   and the Node daemon does not.
2. **Log run boundaries.** Rust writes `--- starting <iso> ---` and
   `--- stopped by user <iso> ---` into the per-tunnel log. TypeScript writes no
   such markers, so the same operations produce different log contents.
3. **Corrupt profile store.** `FileStore::read_profiles` returns an empty list
   on corrupt JSON (boot never fails); the Node store raises `StoreFailed` with
   the path. The daemon therefore reports a broken store while the app silently
   starts empty.
4. **Profile write shape.** Node writes atomically (temp file + rename); Rust
   writes directly with `fs::write`.
5. **Save-time length checks.** Rust `validate_for_save` also caps name (120) and
   target (512) before semantic validation; TypeScript relies on the zod schema
   at the HTTP boundary plus `validateProfileInput`.
6. **Close-session error signal.** HTTP returns `404` when no session is tracked;
   the Tauri command returns `Ok(false)`. The typed client discards both bodies,
   so only HTTP callers can observe `NotFound`.
7. **Health version source.** Rust reports `CARGO_PKG_VERSION`; the daemon
   reports `SERVICE_VERSION`. Both are `0.1.0` today.

**Section sources**

- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L410-L445)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L793-L800)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L986-L991)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L157-L173)
- [crates/tunnel-core/src/host.rs](file://crates/tunnel-core/src/host.rs#L208-L234)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L596-L640)
- [packages/ssh-host-node/src/store.ts](file://packages/ssh-host-node/src/store.ts#L30-L55)
- [packages/ssh-service/src/http.ts](file://packages/ssh-service/src/http.ts#L200-L215)
- [apps/desktop/src-tauri/src/tunnel_commands.rs](file://apps/desktop/src-tauri/src/tunnel_commands.rs#L376-L380)

## Contract Gaps

- **`autoReconnect` is inert.** It is in the zod profile schema, the Rust
  `TunnelProfile`, and the Tauri `ProfileInput`, but no manager or argv builder
  reads it. A stored profile can set it and nothing happens.
- **`mode` is descriptive.** `buildSshArgv`/`build_ssh_argv` derive flags from
  `forwards` and `jumpHops`, never from `mode`. The UI displays and edits it;
  the tunnel does not.
- **Route table is incomplete.** `TUNNEL_ROUTES` has 18 entries; the daemon
  serves 19 endpoints. `DELETE /api/profiles/:id/session` is missing from the
  table and from `routes.ts` path helpers; the client builds that path by hand.
- **Route table is not the server's source.** `http.ts` registers handlers
  literally and the client also uses literal paths for several endpoints, so
  the "declared once" comment in `routes.ts` overstates its authority.
- **No JSON mode in the CLI.** `docs/rust-core.md` plans `--json` everywhere for
  a future `tunnel-cli`; today's `apps/cli` prints human text only.

**Section sources**

- [packages/ssh-contract/src/tunnel.ts](file://packages/ssh-contract/src/tunnel.ts#L96-L108)
- [packages/ssh-contract/src/routes.ts](file://packages/ssh-contract/src/routes.ts#L1-L30)
- [packages/ssh-client/src/index.ts](file://packages/ssh-client/src/index.ts#L270-L280)
- [packages/ssh-core/src/argv.ts](file://packages/ssh-core/src/argv.ts#L85-L135)
- [docs/rust-core.md](file://docs/rust-core.md#L150-L175)

## Recommended Follow-Ups

1. Update `AGENTS.md`: add `crates/tunnel-core`, `packages/web-ui`,
   `packages/build-helper`, `apps/gpui` (or remove gpui), and correct the
   `integrations/*` row to allow the service/host layers for host halves.
2. Extend `check-boundaries.ts` to fail on `as unknown as` in shipped
   `apps/`, `packages/` and `integrations/` code (or narrow the convention to
   tests).
3. Add a Rust-side boundary check (dependency allow-list for `tunnel-core`) and
   a fixture for runtime-record adoption so the two managers cannot diverge
   silently.
4. Add the session-close route to `TUNNEL_ROUTES` and derive both the Hono
   handlers and the client paths from one table.
5. Either implement `autoReconnect`/`mode` semantics or remove them from the
   profile schema and state that they are presentation metadata.

**Section sources**

- [AGENTS.md](file://AGENTS.md#L5-L29)
- [scripts/check-boundaries.ts](file://scripts/check-boundaries.ts#L1-L20)
