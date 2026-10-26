# Build Helper

<cite>
**Referenced Files in This Document**
- [packages/build-helper/package.json](file://packages/build-helper/package.json)
- [packages/build-helper/src/index.ts](file://packages/build-helper/src/index.ts)
- [apps/server/scripts/build.ts](file://apps/server/scripts/build.ts)
- [apps/cli/scripts/build.ts](file://apps/cli/scripts/build.ts)
- [apps/tui/scripts/build.ts](file://apps/tui/scripts/build.ts)
- [apps/web/scripts/build.ts](file://apps/web/scripts/build.ts)
- [apps/desktop/scripts/build.ts](file://apps/desktop/scripts/build.ts)
- [integrations/dsh/scripts/build.ts](file://integrations/dsh/scripts/build.ts)
- [integrations/vscode/scripts/build.ts](file://integrations/vscode/scripts/build.ts)
- [turbo.json](file://turbo.json)
</cite>

## Table of Contents

1. [Purpose](#purpose)
2. [API](#api)
3. [Defaults](#defaults)
4. [TUI Externals](#tui-externals)
5. [Consumers](#consumers)
6. [Why Bun](#why-bun)
7. [Gaps](#gaps)

## Purpose

`@ssh-tunnel/build-helper` centralizes the esbuild configuration used by the
packages that ship a runnable artifact. The comment is explicit: "this module
only holds the common bundling so the six scripts don't drift." Internal
libraries do not use it, because they export TypeScript source directly.

**Section sources**

- [packages/build-helper/src/index.ts](file://packages/build-helper/src/index.ts#L1-L10)

## API

```ts
interface BinaryBuildOptions {
  packageDir: string;   // directory containing src/ and dist/
  entry: string;        // e.g. "src/server.ts"
  outfile: string;      // e.g. "dist/server.mjs"
  external?: readonly string[];
  define?: Record<string, string>;
  jsx?: "automatic" | "transform";
}

buildBinary(options): string          // returns the absolute outfile
TUI_EXTERNALS: readonly string[]
PRODUCTION_DEFINE: Record<string, string>
packageDirFromScriptsFile(scriptsFileUrl): string
```

`buildBinary` resolves `entry`/`outfile` against `packageDir`, creates the output
directory, runs `esbuild.buildSync`, sets the executable bit (`0o755`), and
returns the path. `packageDirFromScriptsFile` goes from a script's
`import.meta.url` up one level (`scripts/` → package root), so each package
owns its build script without hard-coding paths.

**Section sources**

- [packages/build-helper/src/index.ts](file://packages/build-helper/src/index.ts#L14-L72)

## Defaults

| Option | Value |
|---|---|
| entry points | `[packageDir/entry]` |
| bundle | `true` |
| platform | `"node"` |
| format | `"esm"` |
| banner | `#!/usr/bin/env node` |
| loaders | `.tsx → tsx`, `.ts → ts` |
| log level | `info` |
| jsx | only when provided |
| external | only when provided |
| define | only when provided |

`PRODUCTION_DEFINE` is `{ "process.env.NODE_ENV": '"production"' }`.

**Section sources**

- [packages/build-helper/src/index.ts](file://packages/build-helper/src/index.ts#L30-L60)

## TUI Externals

`TUI_EXTERNALS` keeps `react`, `@opentui/core`, `@opentui/react` and every
per-platform `@opentui/core-*` binding out of the bundle. The comment explains
why: "a bundled second React copy crashes every hook at startup, and the native
render binding must resolve from its real pnpm-store location at runtime."

This is a regression fix with tests: `tests/contract/tui-bundle.test.ts` asserts
the bundle exists, contains no embedded React runtime, and keeps the expected
imports. The original failure was `Cannot read properties of null (reading
'useState')` from a null dispatcher.

**Section sources**

- [packages/build-helper/src/index.ts](file://packages/build-helper/src/index.ts#L62-L78)
- [tests/contract/tui-bundle.test.ts](file://tests/contract/tui-bundle.test.ts#L1-L30)

## Consumers

| Package | Uses `buildBinary` | Other work in its script |
|---|---|---|
| `apps/server` | yes (`server.mjs`) | none |
| `apps/cli` | yes (`cli.mjs`) | none |
| `apps/tui` | yes (`tui.mjs`, jsx automatic, TUI externals) | none |
| `apps/web` | no | `vite build` through Bun's `$` |
| `apps/desktop` | no | checks `apps/web/dist`, runs `cargo check` (non-fatal), writes `dist/shell.json` |
| `integrations/dsh` | no | copies `apps/web/dist` to `web/` |
| `integrations/vscode` | no | esbuild CJS with `vscode` external, stages `server.mjs` |

Turbo orders the two artifact-copying builds: DSH build depends on the Web
build, VS Code build depends on the server build. `apps/desktop` has no explicit
dependency on the Web build; its script tolerates a missing `index.html` and the
release workflow runs `pnpm build` before `tauri build`.

**Section sources**

- [turbo.json](file://turbo.json#L1-L45)
- [apps/desktop/scripts/build.ts](file://apps/desktop/scripts/build.ts#L1-L60)
- [integrations/vscode/scripts/build.ts](file://integrations/vscode/scripts/build.ts#L1-L40)

## Why Bun

Every per-package build script starts with `#!/usr/bin/env bun` and runs as
`bun ./scripts/build.ts`. Bun executes TypeScript without a transpile step, so
the scripts can import `@ssh-tunnel/build-helper` source directly. esbuild
still performs the actual bundling; Bun is the runner, not the bundler. CI pins
`bun-version: 1.4.4`.

**Section sources**

- [.github/workflows/ci.yml](file://.github/workflows/ci.yml#L20-L30)
- [apps/tui/scripts/build.ts](file://apps/tui/scripts/build.ts#L1-L10)

## Gaps

1. **No declaration output.** Bundles are `.mjs` with no `.d.ts`; consumers are
   either the same workspace (source imports) or a packaged artifact (no types).
2. **`buildBinary` cannot emit multiple entries or copy assets.** Each package
   scripts around it when it needs more (web, desktop, DSH, VS Code).
3. **No source maps.** `sourcemap` is never set, so a stack trace in a shipped
   bundle points at generated code.
4. **Platform-specific externals are listed manually.** A new OpenTUI target
   must be added to `TUI_EXTERNALS` by hand.

**Section sources**

- [packages/build-helper/src/index.ts](file://packages/build-helper/src/index.ts#L30-L72)
