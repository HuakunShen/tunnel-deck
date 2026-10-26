# Build and Release

<cite>
**Referenced Files in This Document**
- [package.json](file://package.json)
- [turbo.json](file://turbo.json)
- [pnpm-workspace.yaml](file://pnpm-workspace.yaml)
- [.github/workflows/ci.yml](file://.github/workflows/ci.yml)
- [.github/workflows/release.yml](file://.github/workflows/release.yml)
- [scripts/set-app-version.mjs](file://scripts/set-app-version.mjs)
- [scripts/app-version.mjs](file://scripts/app-version.mjs)
- [scripts/collect-bundles.mjs](file://scripts/collect-bundles.mjs)
- [apps/desktop/src-tauri/tauri.conf.json](file://apps/desktop/src-tauri/tauri.conf.json)
- [apps/desktop/src-tauri/Cargo.toml](file://apps/desktop/src-tauri/Cargo.toml)
</cite>

## Table of Contents

1. [Local Commands](#local-commands)
2. [Turbo Graph](#turbo-graph)
3. [CI](#ci)
4. [Release Workflow](#release-workflow)
5. [Version Stamping](#version-stamping)
6. [Artifact Naming](#artifact-naming)
7. [Signing Reality](#signing-reality)

## Local Commands

| Script | Effect |
|---|---|
| `pnpm install` | pnpm 12.8.1, Node >= 20 (CI uses 24) |
| `pnpm check` | `tsc --noEmit` (or svelte-check) in every workspace package |
| `pnpm check:boundaries` | `bun scripts/check-boundaries.ts` |
| `pnpm test` | all vitest suites, including TUI snapshots |
| `pnpm test:ci` | vitest excluding `apps/tui/**` |
| `pnpm test:e2e` | vitest with `vitest.e2e.config.ts` (real pty) |
| `pnpm build` | `turbo run build` |
| `pnpm serve` | `node apps/server/dist/server.mjs` |
| `pnpm tui` | `node apps/tui/dist/tui.mjs` |
| `pnpm format` | Prettier over ts/tsx/md/json/yaml |

`pnpm build` requires Bun because each package's `build` script is a Bun
TypeScript file. `pnpm check`, `test`, `serve` and `tui` only need Node.

**Section sources**

- [package.json](file://package.json#L1-L35)

## Turbo Graph

- `build` depends on `^build` (workspace dependency order), inputs
  `$TURBO_DEFAULT$`, outputs `dist/**` and `web/**`.
- `check` depends on `^check`, no outputs.
- `dsh-plugin-ssh-tunnel#build` depends on `@ssh-tunnel/web#build` — it copies
  the built console into `web/`.
- `vscode-ssh-tunnel#build` depends on `@ssh-tunnel/server#build` — it copies
  `server.mjs` beside the extension.

No other package declares an output dependency. In particular `apps/desktop`
does not depend on `@ssh-tunnel/web#build`; its build script only checks for the
presence of `apps/web/dist` and warns if missing. The release workflow avoids
the race by running `pnpm build` before `tauri build`.

**Section sources**

- [turbo.json](file://turbo.json#L1-L45)
- [apps/desktop/scripts/build.ts](file://apps/desktop/scripts/build.ts#L1-L30)

## CI

`.github/workflows/ci.yml` has three jobs:

### `check` (macos-latest)

1. checkout with submodules;
2. Bun 1.4.4, pnpm action, Node 24 with pnpm cache;
3. `pnpm install --frozen-lockfile`;
4. `pnpm check`;
5. `pnpm check:boundaries`;
6. `pnpm build` (needed so the DSH panel document and TUI bundle exist);
7. `pnpm test:ci`.

### `core` (ubuntu-22.04)

Stable Rust + clippy + `Swatinem/rust-cache`:

- `cargo test -p tunnel-core`;
- `cargo clippy -p tunnel-core --all-targets -- -D warnings`.

The comment explains why Ubuntu is enough: the crate is WebView-free by
construction, and the shell matrix covers OS-specific branches.

### `desktop` (macOS, Ubuntu, Windows)

- installs Linux WebKitGTK/AppIndicator/librsvg/patchelf prerequisites;
- stubs `apps/web/dist/index.html` and `apps/server/dist/server.mjs` because
  `generate_context!` only requires the paths to exist;
- `cargo check --workspace` in `apps/desktop/src-tauri`;
- `cargo test` only on macOS.

**Section sources**

- [.github/workflows/ci.yml](file://.github/workflows/ci.yml#L1-L130)

## Release Workflow

Triggered by tags matching `v*` or manually. `permissions: contents: write`.
Global `MACOSX_DEPLOYMENT_TARGET: "11.0"` so the Intel build launches on older
systems.

Matrix:

| Label | Runner | Tauri args | Target |
|---|---|---|---|
| `macos-arm64` | macos-latest | `--target aarch64-apple-darwin --bundles app,dmg` | aarch64 |
| `macos-x64` | macos-latest | `--target x86_64-apple-darwin --bundles app,dmg` | x86_64 |
| `linux-x64` | ubuntu-22.04 | `--bundles deb,appimage` | — |
| `windows-x64` | windows-latest | `--bundles nsis` | — |

Each job installs Bun/pnpm/Node/Rust, builds the workspace, stamps the version
from the tag (when tagged), reads the resolved version, runs
`pnpm exec tauri build <args>`, collects bundles, zips the `.app` on macOS, and
uploads workflow artifacts.

The `publish` job runs only for tags, downloads all artifacts into one `dist/`,
lists them, and creates **one** GitHub Release with every file and generated
notes.

`tauri.conf.json` alone declares `targets: ["app"]`; the workflow's
`--bundles` flags are what produce `.dmg`, `.deb`, `.AppImage` and the NSIS
installer.

**Section sources**

- [.github/workflows/release.yml](file://.github/workflows/release.yml#L1-L180)

## Version Stamping

Two independent scripts keep tag and bundle honest:

- `scripts/set-app-version.mjs <vX.Y.Z>` validates semver (optional `v` prefix)
  and rewrites `apps/desktop/src-tauri/tauri.conf.json`'s `version`; if it is
  already equal, it does nothing.
- `scripts/app-version.mjs` prints the version **from that same file**, so the
  release artifact name and the bundle's `CFBundleShortVersionString` cannot
  disagree.

The workflow stamps only for `refs/tags/v*`; a manual run uses whatever version
is already in the config.

**Section sources**

- [scripts/set-app-version.mjs](file://scripts/set-app-version.mjs#L1-L40)
- [scripts/app-version.mjs](file://scripts/app-version.mjs#L1-L25)
- [apps/desktop/src-tauri/tauri.conf.json](file://apps/desktop/src-tauri/tauri.conf.json#L1-L8)

## Artifact Naming

Tauri names artifacts in native-installer style (`SSH Tunnel_0.1.3_aarch64.dmg`,
`SSH Tunnel_0.1.3_x64-setup.exe`). `scripts/collect-bundles.mjs` walks the bundle
tree and copies each recognized extension to
`dist/SSH-Tunnel-<version>-<label><suffix><ext>`:

| Extension | Suffix |
|---|---|
| `.dmg`, `.deb`, `.AppImage`, `.msi` | (none) |
| `.exe` | `-setup` |

It fails when the bundle directory is missing or nothing matched. macOS also
gets a `.zip` created with `ditto -c -k --sequesterRsrc --keepParent`, because a
browser or script may refuse a disk image.

**Section sources**

- [scripts/collect-bundles.mjs](file://scripts/collect-bundles.mjs#L1-L90)

## Signing Reality

- macOS builds are **ad-hoc signed** (`"signingIdentity": "-"`), not notarized.
  A downloaded copy carries the quarantine flag; the README tells users to
  right-click > Open or strip it with `xattr`.
- Windows installers are unsigned; SmartScreen requires "More info > Run anyway".
- The release comments list the secrets that would enable real signing
  (`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`,
  `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`) and
  explain why an **empty** `APPLE_CERTIFICATE` is worse than an absent one: Tauri
  would try to import nothing and fail.

Ad-hoc signing also means the macOS Local Network privacy grant has no stable
app identity, which is why the README says the prompt may not stick.

**Section sources**

- [.github/workflows/release.yml](file://.github/workflows/release.yml#L90-L120)
- [apps/desktop/src-tauri/tauri.conf.json](file://apps/desktop/src-tauri/tauri.conf.json#L40-L60)
- [README.md](file://README.md#L20-L46)
