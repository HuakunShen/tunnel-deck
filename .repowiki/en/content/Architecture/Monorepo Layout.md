# Monorepo Layout

<cite>
**Referenced Files in This Document**
- [pnpm-workspace.yaml](file://pnpm-workspace.yaml)
- [package.json](file://package.json)
- [turbo.json](file://turbo.json)
- [tsconfig.base.json](file://tsconfig.base.json)
- [Cargo.toml](file://Cargo.toml)
- [apps/desktop/src-tauri/Cargo.toml](file://apps/desktop/src-tauri/Cargo.toml)
- [apps/gpui/Cargo.toml](file://apps/gpui/Cargo.toml)
- [vitest.config.ts](file://vitest.config.ts)
- [scripts/check-boundaries.ts](file://scripts/check-boundaries.ts)
- [fixtures/argv/control.json](file://fixtures/argv/control.json)
- [.gitmodules](file://.gitmodules)
- [.gitignore](file://.gitignore)
</cite>

## Table of Contents

1. [Workspace Systems](#workspace-systems)
2. [pnpm Members](#pnpm-members)
3. [Dependency Edges](#dependency-edges)
4. [Rust Workspaces](#rust-workspaces)
5. [Repository Directories](#repository-directories)
6. [Source-Direct Packages and Bundles](#source-direct-packages-and-bundles)
7. [Version and Script Ownership](#version-and-script-ownership)
8. [Generated and Untracked Trees](#generated-and-untracked-trees)

## Workspace Systems

The repository is a pnpm/Turborepo monorepo *and* three separate Cargo
workspaces. The split is structural, not cosmetic:

- `pnpm-workspace.yaml` includes `apps/*`, `packages/*`, `integrations/*`,
  `tests/*`, and `spikes/*`. There are no `package.json` files under `tests/`,
  so the effective set is 14 published-name packages plus one spike.
- Root `Cargo.toml` has `members = ["crates/*"]` and
  `default-members = ["crates/tunnel-core"]`. Its header states the reason:
  the Tauri shell and the GPUI experiment are separate workspaces so a root
  `cargo build` never needs a WebView.
- `apps/desktop/src-tauri` declares `[workspace]` with no members, isolating
  Tauri/WebView dependencies.
- `apps/gpui` declares its own workspace with `tunnel-console` and `tunnel-api`.

**Section sources**

- [pnpm-workspace.yaml](file://pnpm-workspace.yaml#L1-L12)
- [Cargo.toml](file://Cargo.toml#L1-L22)
- [apps/desktop/src-tauri/Cargo.toml](file://apps/desktop/src-tauri/Cargo.toml#L1-L15)
- [apps/gpui/Cargo.toml](file://apps/gpui/Cargo.toml#L1-L30)

## pnpm Members

| Path | Package name | Role |
|---|---|---|
| `packages/ssh-contract` | `@ssh-tunnel/contract` | zod schemas, route table, closed error codes |
| `packages/ssh-core` | `@ssh-tunnel/core` | argv builder, validation, state machine, endpoints |
| `packages/ssh-client` | `@ssh-tunnel/client` | typed fetch client + Tauri invoke client |
| `packages/ssh-host-node` | `@ssh-tunnel/host-node` | Node implementations of the ports |
| `packages/ssh-service` | `@ssh-tunnel/service` | `TunnelManager`, Hono app, `startDaemon` |
| `packages/web-ui` | `@ssh-tunnel/web-ui` | shadcn-svelte components + shared stylesheet |
| `packages/build-helper` | `@ssh-tunnel/build-helper` | shared esbuild bundling for runnable artifacts |
| `apps/server` | `@ssh-tunnel/server` | `ssh-tunnel-serve` daemon binary |
| `apps/cli` | `@ssh-tunnel/cli` | `ssh-tunnel` CLI (daemon or `--direct`) |
| `apps/tui` | `@ssh-tunnel/tui` | OpenTUI/React fullscreen terminal UI |
| `apps/web` | `@ssh-tunnel/web` | SvelteKit static console |
| `apps/desktop` | `@ssh-tunnel/desktop` | Tauri shell + WebDriver e2e |
| `integrations/dsh` | `dsh-plugin-ssh-tunnel` | DeepSeek Harness host+client halves |
| `integrations/vscode` | `vscode-ssh-tunnel` | VS Code extension |
| `spikes/uniview-tui` | spike | uniview evaluation, never shipped |

**Section sources**

- [pnpm-workspace.yaml](file://pnpm-workspace.yaml#L1-L12)
- [README.md](file://README.md#L63-L78)

## Dependency Edges

Actual workspace dependencies, read from each `package.json`:

```text
ssh-contract ──► (zod)
ssh-core     ──► ssh-contract
ssh-client   ──► ssh-contract
ssh-host-node──► ssh-contract, ssh-core
ssh-service  ──► ssh-contract, ssh-core, ssh-host-node, hono, @hono/node-server
web-ui       ──► bits-ui, clsx, tailwind-merge, tailwind-variants (svelte peer)
build-helper ──► esbuild

server ──► ssh-service
cli    ──► ssh-client, ssh-contract, ssh-host-node, ssh-service
tui    ──► ssh-client, ssh-contract, ssh-core, ssh-host-node, ssh-service, react, @opentui/*
web    ──► ssh-client, ssh-contract, ssh-core, ssh-web-ui, @lucide/svelte
desktop──► ssh-client, ssh-contract        (the Rust shell consumes crates/tunnel-core)
dsh    ──► ssh-host-node, ssh-service
vscode ──► ssh-client, ssh-contract
```

`integrations/dsh` is the one place that contradicts `AGENTS.md`'s import
table, which allows integrations only `ssh-contract` and `ssh-client`. The
plugin in fact imports `TunnelManager`, `createServiceApp` and all Node host
ports. See [Boundaries and Drift](Boundaries%20and%20Drift.md).

**Section sources**

- [integrations/dsh/package.json](file://integrations/dsh/package.json#L20-L35)
- [integrations/dsh/src/host.ts](file://integrations/dsh/src/host.ts#L30-L60)
- [apps/web/package.json](file://apps/web/package.json#L14-L30)

## Rust Workspaces

### Root (headless)

`members = ["crates/*"]` contains only `crates/tunnel-core`. Shared workspace
dependencies are `serde`, `serde_json`, `thiserror`, `chrono` (clock + std) and
`libc`. There is no tokio, axum, Tauri or HTTP dependency in this workspace.

`tunnel-core` modules: `argv`, `contract`, `error`, `host`, `manager`, `ports`,
`state_machine`, `validate`; plus an integration test `tests/fixtures.rs`.

### Desktop shell

`apps/desktop/src-tauri` depends on `tauri 2`, `tauri-plugin-shell`,
`tauri-plugin-dialog`, `tunnel-core` by path, and release-only
`tauri-plugin-wdio-webdriver`. Platform deps add `window-vibrancy` plus
`objc2`/`objc2-app-kit`/`objc2-foundation` on macOS and `window-vibrancy` on
Windows. Release profile: `opt-level = "s"`, thin LTO, one codegen unit.

### GPUI experiment (untracked)

`apps/gpui` is a separate workspace with `tunnel-api` (serde + ureq HTTP client)
and `tunnel-console` (gpui 0.2.2 binary `ssh-tunnel-gpui`, plus the
`applications` crate to read installed app icons). It is not referenced by the
root Cargo workspace, pnpm, turbo, CI, or git history.

**Section sources**

- [Cargo.toml](file://Cargo.toml#L1-L22)
- [crates/tunnel-core/Cargo.toml](file://crates/tunnel-core/Cargo.toml#L1-L27)
- [apps/desktop/src-tauri/Cargo.toml](file://apps/desktop/src-tauri/Cargo.toml#L25-L70)
- [apps/gpui/crates/tunnel-api/Cargo.toml](file://apps/gpui/crates/tunnel-api/Cargo.toml#L1-L20)

## Repository Directories

| Directory | Purpose | Committed |
|---|---|---|
| `apps/` | runnable fronts | yes (gpui is not) |
| `packages/` | libraries and service layer | yes |
| `crates/` | Rust core | yes |
| `integrations/` | editor/harness frontends | yes |
| `tests/` | core/contract/e2e suites (no package.json) | yes |
| `fixtures/` | cross-language golden argv and transitions | yes |
| `scripts/` | boundary check, version stamping, bundle collection | yes |
| `docs/` | design notes and spike reports | yes |
| `vendors/uniview` | git submodule for co-evolution | yes (submodule) |
| `spikes/uniview-tui` | evaluation only, never imported by apps | yes |
| `assets/`, `screenshots/` | branding and gallery images | yes |
| `apps/gpui/` | GPUI experiment | **no** |

`vendors/uniview` is the only submodule (`gitmodules`), pinned by the root
commit. `spikes/*` is in the pnpm workspace but explicitly "Experimental spikes
only; never shipped, never imported by apps."

**Section sources**

- [.gitmodules](file://.gitmodules#L1-L3)
- [pnpm-workspace.yaml](file://pnpm-workspace.yaml#L1-L8)
- [README.md](file://README.md#L63-L78)

## Source-Direct Packages and Bundles

Internal libraries do not build. Every `package.json` exports TypeScript
source directly (`"." : "./src/index.ts"`), and `vitest.config.ts` aliases the
package names to the same `src/index.ts` files. This is why `pnpm check` is a
plain `tsc --noEmit` per package and why cross-package tests exercise shipped
code.

Only runnable artifacts are bundled, through `@ssh-tunnel/build-helper`:

| Artifact | Entry | Output | Externals |
|---|---|---|---|
| daemon | `apps/server/src/server.ts` | `dist/server.mjs` | none |
| CLI | `apps/cli/src/cli.ts` | `dist/cli.mjs` | none |
| TUI | `apps/tui/src/tui.tsx` | `dist/tui.mjs` | react + all `@opentui/*` platform packages |
| Web | SvelteKit | `apps/web/dist/` | n/a (Vite build) |
| Desktop manifest | build script | `apps/desktop/dist/shell.json` | n/a |
| DSH | script | staged `integrations/dsh/web/` | n/a |
| VS Code | `extension.ts` | `dist/extension.js` (CJS) + staged `server.mjs` | `vscode` |

Build scripts run with Bun (`bun ./scripts/build.ts`), because Bun executes
TypeScript directly; esbuild does the bundling. The root scripts still run the
artifacts with Node (`node apps/server/dist/server.mjs`).

**Section sources**

- [packages/build-helper/src/index.ts](file://packages/build-helper/src/index.ts#L1-L72)
- [apps/tui/scripts/build.ts](file://apps/tui/scripts/build.ts#L1-L25)
- [integrations/vscode/scripts/build.ts](file://integrations/vscode/scripts/build.ts#L1-L40)
- [vitest.config.ts](file://vitest.config.ts#L1-L30)

## Version and Script Ownership

- Third-party versions live only in `pnpm-workspace.yaml`'s `catalog:` block;
  packages request `catalog:`. Exceptions are a few dev tools pinned directly
  in the root (`prettier`) and integration ranges (`@types/vscode`).
- `allowBuilds` permits install scripts for `esbuild`, `edgedriver` and
  `geckodriver` only.
- `minimumReleaseAgeExclude` lists exact versions allowed despite a release-age
  policy (OpenTUI, Lucide, Tauri CLI).
- Turbo tasks: `build` depends on `^build`; `dsh-plugin-ssh-tunnel#build`
  depends on `@ssh-tunnel/web#build`; `vscode-ssh-tunnel#build` depends on
  `@ssh-tunnel/server#build`. Those two explicit edges are what make the
  staged artifacts exist before the plugins copy them.

**Section sources**

- [pnpm-workspace.yaml](file://pnpm-workspace.yaml#L13-L75)
- [turbo.json](file://turbo.json#L1-L45)
- [package.json](file://package.json#L1-L40)

## Generated and Untracked Trees

`.gitignore` excludes `node_modules/`, `dist/`, `.turbo/`, `.svelte-kit/`,
`target/`, `*.log`, `.DS_Store`, and `integrations/dsh/web/`. Everything under
those paths in a working checkout is generated, including the DSH panel
document that `tests/contract/dsh.test.ts` serves.

At the time of writing, `git status` reports exactly two untracked entries:
`apps/gpui/` and `screenshots/live-desktop.png`. `apps/gpui` is therefore
describable code, but not part of the repository's committed state, builds or
releases.

**Section sources**

- [.gitignore](file://.gitignore#L1-L10)
- [.github/workflows/ci.yml](file://.github/workflows/ci.yml#L45-L60)
