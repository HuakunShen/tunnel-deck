# Testing and Fixtures

<cite>
**Referenced Files in This Document**
- [vitest.config.ts](file://vitest.config.ts)
- [vitest.e2e.config.ts](file://vitest.e2e.config.ts)
- [fixtures/state-machine.json](file://fixtures/state-machine.json)
- [fixtures/argv/basic-local.json](file://fixtures/argv/basic-local.json)
- [tests/core/fixtures.test.ts](file://tests/core/fixtures.test.ts)
- [tests/contract/http.test.ts](file://tests/contract/http.test.ts)
- [tests/contract/multiplex.test.ts](file://tests/contract/multiplex.test.ts)
- [tests/contract/runtime-store.test.ts](file://tests/contract/runtime-store.test.ts)
- [tests/contract/dsh.test.ts](file://tests/contract/dsh.test.ts)
- [tests/contract/vscode.test.ts](file://tests/contract/vscode.test.ts)
- [tests/contract/shell-geometry.test.ts](file://tests/contract/shell-geometry.test.ts)
- [tests/contract/tui-bundle.test.ts](file://tests/contract/tui-bundle.test.ts)
- [tests/e2e/tui.test.ts](file://tests/e2e/tui.test.ts)
- [crates/tunnel-core/tests/fixtures.rs](file://crates/tunnel-core/tests/fixtures.rs)
- [.github/workflows/ci.yml](file://.github/workflows/ci.yml)
</cite>

## Table of Contents

1. [Runners](#runners)
2. [Suite Map](#suite-map)
3. [Cross-Language Fixtures](#cross-language-fixtures)
4. [Rust Gates](#rust-gates)
5. [Desktop e2e](#desktop-e2e)
6. [What CI Actually Runs](#what-ci-actually-runs)
7. [Known Failures and Gaps](#known-failures-and-gaps)

## Runners

Vitest is the only JavaScript test runner. Two configs:

- `vitest.config.ts`: aliases every workspace package to its `src/index.ts` (so
  tests exercise source, not `dist`), includes
  `tests/**/*.test.ts(x)`, `packages/*/src/**/*.test.ts`,
  `apps/*/src/**/*.test.tsx` and `spikes/*/src/**/*.test.tsx`, and excludes
  `tests/e2e/**`.
- `vitest.e2e.config.ts`: includes only `tests/e2e/**/*.test.ts`, with 120 s
  test/hook timeouts, and runs via `pnpm test:e2e`.

Aliases include integration entry points (`dsh-plugin-ssh-tunnel`,
`vscode-ssh-tunnel/supervisor`, `vscode-ssh-tunnel/tree`) and the TUI view, so
those can be tested without publishing.

**Section sources**

- [vitest.config.ts](file://vitest.config.ts#L1-L40)
- [vitest.e2e.config.ts](file://vitest.e2e.config.ts#L1-L15)

## Suite Map

| Suite | Files | Focus |
|---|---|---|
| core | `tests/core/argv.test.ts`, `endpoint.test.ts`, `validate.test.ts`, `fixtures.test.ts` | pure domain: flags, endpoints, semantics, state machine, golden fixtures |
| service/contract | `tests/contract/http.test.ts` | Hono app against an in-memory manager with fake ports: CRUD, validation, start/stop, probe, logs, launch, open-url, hosts, status mapping |
| host contract | `tests/contract/which.test.ts`, `runtime-store.test.ts` | Windows/POSIX PATH resolution; runtime-file degradation |
| platform behaviour | `tests/contract/multiplex.test.ts` | no-ControlMaster argv and pid adoption after restart |
| integrations | `tests/contract/dsh.test.ts`, `vscode.test.ts` | loopback guards + proxy; supervisor attach/spawn and tree rendering |
| UI contracts | `tests/contract/shell-geometry.test.ts`, `tui-bundle.test.ts` | shell dimensions from CSS; TUI bundle has no second React |
| TUI unit | `apps/tui/src/view.test.tsx`, `form.test.tsx` | memory-frame snapshots and form validation |
| e2e TUI | `tests/e2e/tui.test.ts` | real pty via `@microsoft/tui-test` |
| desktop e2e | `apps/desktop/e2e/specs/app.js` | WebdriverIO against the bundled app |

**Section sources**

- [tests/contract/http.test.ts](file://tests/contract/http.test.ts#L1-L30)
- [tests/contract/multiplex.test.ts](file://tests/contract/multiplex.test.ts#L1-L30)
- [tests/contract/runtime-store.test.ts](file://tests/contract/runtime-store.test.ts#L1-L30)
- [tests/contract/dsh.test.ts](file://tests/contract/dsh.test.ts#L1-L20)
- [tests/contract/vscode.test.ts](file://tests/contract/vscode.test.ts#L1-L20)

## Cross-Language Fixtures

`fixtures/` is the only mechanism keeping the Rust and TypeScript domain
implementations aligned.

`fixtures/argv/*.json` — six files (`basic-local`, `remote`, `dynamic`, `jump`,
`control`, `interactive`). Each carries:

- `name` (human description);
- a full `profile` with `id`, `mode`, `sshTarget`, `forwards`, timestamps;
- optional `options` (`controlPath`, `interactive`);
- `mustContain` and `mustNotContain` substrings of the joined argv.

`fixtures/state-machine.json` carries 19 transitions `{from, to, allowed}`
covering every accepted and rejected pair.

Both `tests/core/fixtures.test.ts` and
`crates/tunnel-core/tests/fixtures.rs` read these files and assert their own
implementation. A domain change must update the fixture first, so the other
language fails loudly.

**Section sources**

- [fixtures/argv/basic-local.json](file://fixtures/argv/basic-local.json#L1-L15)
- [fixtures/state-machine.json](file://fixtures/state-machine.json#L1-L24)
- [tests/core/fixtures.test.ts](file://tests/core/fixtures.test.ts#L1-L55)
- [crates/tunnel-core/tests/fixtures.rs](file://crates/tunnel-core/tests/fixtures.rs#L1-L99)

## Rust Gates

- Unit tests live inside each `tunnel-core` module (`contract`, `error`,
  `state_machine`, `validate`, `argv`, `host`, `manager`).
- `manager.rs` tests use fake implementations of all eight ports, so the
  lifecycle runs without spawning `ssh` (the 900 ms grace is real, so one test
  pays for it).
- `tests/fixtures.rs` is the cross-language gate.
- CI: `cargo test -p tunnel-core` and
  `cargo clippy -p tunnel-core --all-targets -- -D warnings`.

There is no Rust test for real process spawning, terminal launching or file
locking; those are exercised only through the desktop app.

**Section sources**

- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L1221-L1537)
- [.github/workflows/ci.yml](file://.github/workflows/ci.yml#L60-L85)

## Desktop e2e

`apps/desktop/e2e` uses WebdriverIO + `@wdio/tauri-service` with the app's
embedded WebDriver server:

- `SSH_TUNNEL_WEBDRIVER=1` is required to register the plugin (the config sets
  it by default);
- `appBinaryPath` is hard-coded to the macOS release bundle, so the suite is
  macOS-only;
- specs assert the title, the three shell regions, the "New tunnel" button, the
  published native material (`glass`/`vibrancy`) and the `--material-opacity`
  knob, then capture `screenshots/desktop-app.png`.

It is not part of CI; it runs on a machine that has built the app
(`pnpm --filter @ssh-tunnel/desktop build:app`).

**Section sources**

- [apps/desktop/e2e/wdio.conf.ts](file://apps/desktop/e2e/wdio.conf.ts#L1-L60)
- [apps/desktop/e2e/specs/app.js](file://apps/desktop/e2e/specs/app.js#L1-L75)

## What CI Actually Runs

The `check` job runs, in order:

1. `pnpm check` — every package's type/svelte check;
2. `pnpm check:boundaries`;
3. `pnpm build` — required because `tui-bundle.test.ts` reads
   `apps/tui/dist/tui.mjs` and the DSH test serves the staged panel document;
4. `pnpm test:ci` — `vitest run --exclude 'apps/tui/**'`.

The `core` job runs the Rust tests and clippy. The `desktop` job runs
`cargo check --workspace` on three OSes and `cargo test` on macOS only.

**Section sources**

- [.github/workflows/ci.yml](file://.github/workflows/ci.yml#L14-L130)
- [package.json](file://package.json#L20-L30)

## Known Failures and Gaps

1. **TUI snapshots are excluded from CI.** The README records that four
   render-snapshot assertions fail under this vitest setup because OpenTUI's
   native FFI is unavailable; `pnpm test:ci` avoids the whole `apps/tui`
   directory. The pure view/form tests therefore do not gate merges.
2. **No coverage measurement.** No coverage provider, thresholds or reports are
   configured.
3. **No Web UI component tests.** `apps/web` is covered by svelte-check, the
   desktop e2e selectors and the geometry test, but not by DOM-level tests.
4. **No test for real terminal launch, real `ssh`, or real file permissions**
   beyond the runtime-store mode assertion.
5. **Build-order coupling.** Tests that read `dist/` fail on a clean checkout
   unless `pnpm build` ran first; CI encodes that order in a comment.
6. **No cross-language fixture for persistence, adoption, probing or log
   reading**, which is where the two managers actually differ (see
   [Boundaries and Drift](../Architecture/Boundaries%20and%20Drift.md)).
7. **Desktop e2e is macOS-only and not in CI**, so the Windows/Linux shells are
   compile-checked but never run headlessly.

**Section sources**

- [README.md](file://README.md#L146-L151)
- [tests/contract/tui-bundle.test.ts](file://tests/contract/tui-bundle.test.ts#L1-L20)
- [.github/workflows/ci.yml](file://.github/workflows/ci.yml#L36-L45)
