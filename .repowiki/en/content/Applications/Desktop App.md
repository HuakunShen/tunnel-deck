# Desktop App

<cite>
**Referenced Files in This Document**
- [apps/desktop/package.json](file://apps/desktop/package.json)
- [apps/desktop/src-tauri/Cargo.toml](file://apps/desktop/src-tauri/Cargo.toml)
- [apps/desktop/src-tauri/src/lib.rs](file://apps/desktop/src-tauri/src/lib.rs)
- [apps/desktop/src-tauri/src/main.rs](file://apps/desktop/src-tauri/src/main.rs)
- [apps/desktop/src-tauri/src/tunnel_commands.rs](file://apps/desktop/src-tauri/src/tunnel_commands.rs)
- [apps/desktop/src-tauri/src/native_material.rs](file://apps/desktop/src-tauri/src/native_material.rs)
- [apps/desktop/src-tauri/src/native_material.js](file://apps/desktop/src-tauri/src/native_material.js)
- [apps/desktop/src-tauri/tauri.conf.json](file://apps/desktop/src-tauri/tauri.conf.json)
- [apps/desktop/src-tauri/Info.plist](file://apps/desktop/src-tauri/Info.plist)
- [apps/desktop/scripts/build.ts](file://apps/desktop/scripts/build.ts)
- [apps/desktop/e2e/wdio.conf.ts](file://apps/desktop/e2e/wdio.conf.ts)
- [apps/desktop/e2e/specs/app.js](file://apps/desktop/e2e/specs/app.js)
- [apps/desktop/src/index.ts](file://apps/desktop/src/index.ts)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs)
</cite>

## Table of Contents

1. [What the Shell Is](#what-the-shell-is)
2. [Process Model](#process-model)
3. [Command Surface](#command-surface)
4. [Window Creation and Native Material](#window-creation-and-native-material)
5. [Configuration](#configuration)
6. [Build and e2e](#build-and-e2e)
7. [Gaps and Drift](#gaps-and-drift)

## What the Shell Is

`apps/desktop` is a Tauri 2 shell around the same Svelte console the daemon
serves. The shell links `crates/tunnel-core` directly, so the installed app
needs no Node runtime, no sidecar process and no loopback HTTP port. Commit
`85d04e0` ("link tunnel-core in-process and drop the node sidecar") is the
change that made this true; earlier history spawned `node server.mjs`.

The only JavaScript-facing Rust surface is a set of `#[tauri::command]`
functions. The console reaches them through
`createInvokeTunnelClient` in `@ssh-tunnel/client`.

**Section sources**

- [apps/desktop/src-tauri/src/lib.rs](file://apps/desktop/src-tauri/src/lib.rs#L1-L30)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L1-L8)
- [apps/desktop/src/index.ts](file://apps/desktop/src/index.ts#L1-L24)

## Process Model

`app_state()` builds the exact manager composition once:

```rust
Manager<
  OsProcess, TcpProbe, FileStore, SystemClock,
  FileRuntimeStore, OsTerminal, OsOpenUrl, SshControl
>
```

Paths come from `default_base_dir()` (`~/.ssh-tunnel-ui`): `profiles.json`,
`runtime.json`, `logs/`, `control/`. `ssh` is resolved by name, not an absolute
path. `SshControl` is installed only when `supports_control_master()` is true
(POSIX), which is what makes Windows ownership pid-based.

The manager sits behind `Mutex<AppManager>`; every command locks it. Commands
are synchronous so `start`'s grace sleep and `stop`'s kill grace run on Tauri's
command thread pool, not the UI thread. The consequence is that concurrent
commands serialize: a 30 s start blocks a status poll until it finishes.

`instance_id` is `app-<unix millis>-<pid>`, reported by `tunnel_health`.

**Section sources**

- [apps/desktop/src-tauri/src/tunnel_commands.rs](file://apps/desktop/src-tauri/src/tunnel_commands.rs#L26-L71)
- [apps/desktop/src-tauri/src/tunnel_commands.rs](file://apps/desktop/src-tauri/src/tunnel_commands.rs#L218-L232)

## Command Surface

19 commands are registered in `lib.rs`:

`tunnel_health`, `tunnel_list_profiles`, `tunnel_create_profile`,
`tunnel_get_profile`, `tunnel_update_profile`, `tunnel_delete_profile`,
`tunnel_list_states`, `tunnel_get_state`, `tunnel_preview`, `tunnel_start`,
`tunnel_stop`, `tunnel_stop_all`, `tunnel_probe`, `tunnel_log_tail`,
`tunnel_list_hosts`, `tunnel_list_terminals`, `tunnel_launch_in_terminal`,
`tunnel_close_session`, `tunnel_open_url`.

Each is a thin adapter: deserialize a typed request, call one manager method,
serialize the response. List responses use the same envelopes as HTTP
(`ProfileList`, `StateList`, `HostList`, `TerminalList`) so the shared client
validates IPC and HTTP with the same schemas.

`ProfileInput` is a **second, hand-written copy** of the create/update shape
(serde `camelCase`), not generated from the zod contract. That is the main
schema-drift risk in this front.

Errors serialize as `CommandProblem { code, message, retryable }`. The
`code` enum is `TunnelProblemCode`, whose serde spelling matches the TypeScript
table exactly.

**Section sources**

- [apps/desktop/src-tauri/src/lib.rs](file://apps/desktop/src-tauri/src/lib.rs#L38-L72)
- [apps/desktop/src-tauri/src/tunnel_commands.rs](file://apps/desktop/src-tauri/src/tunnel_commands.rs#L73-L217)
- [apps/desktop/src-tauri/src/tunnel_commands.rs](file://apps/desktop/src-tauri/src/tunnel_commands.rs#L292-L350)

## Window Creation and Native Material

The window is configured with `create: false` and built by hand in
`native_material::create_main_window` so two things exist before the first
paint:

1. the initialization script that publishes the window material;
2. the OS material itself.

If material installation fails, `lib.rs` falls back to
`WebviewWindowBuilder::from_config` and prints a warning; the window is then
opaque but usable. After setup, `app.get_webview_window("main").show()` reveals
it (`visible: false` in config avoids a white flash).

### macOS

`macos::install` runs inside `with_webview`:

- casts the platform webview to `NSWindow`/`NSView`;
- requires `_setDrawsBackground:` to exist, otherwise stays opaque;
- tries `apply_liquid_glass` (macOS 26+, detected by looking up
  `NSGlassEffectView`) and falls back to `apply_vibrancy`
  (`UnderWindowBackground`);
- makes the window and webview transparent, clears `underPageBackgroundColor`
  when supported, and sets `titlebarAppearsTransparent`;
- inspects the content view hierarchy to report what is actually installed
  (`glass`, `vibrancy`, or `none`) rather than assuming success;
- publishes after a 400 ms blocker-thread delay, because `eval` during setup
  deadlocks an already-running webview.

### Windows

`windows::install` tries `apply_mica` then `apply_acrylic`, publishing
`vibrancy` or `none`. It passes the `WebviewWindow` (which implements
`HasWindowHandle`) instead of a raw `HWND` from a possibly different `windows`
crate version.

### Page contract

`native_material.js` defines `window.__nativeHost.apply(backdrop)`, sets
`document.documentElement.dataset.backdrop`, and dispatches a `nativehost`
event. The console only enables translucent tokens when the value is `glass`
or `vibrancy`; a plain browser never runs the script.

**Section sources**

- [apps/desktop/src-tauri/src/lib.rs](file://apps/desktop/src-tauri/src/lib.rs#L20-L45)
- [apps/desktop/src-tauri/src/native_material.rs](file://apps/desktop/src-tauri/src/native_material.rs#L1-L188)
- [apps/desktop/src-tauri/src/native_material.js](file://apps/desktop/src-tauri/src/native_material.js#L1-L30)
- [apps/web/src/routes/+page.svelte](file://apps/web/src/routes/+page.svelte#L60-L80)

## Configuration

`tauri.conf.json`:

| Key | Value |
|---|---|
| productName | `SSH Tunnel` |
| version | `0.1.3` |
| identifier | `dev.sshtunnel.ui` |
| frontendDist | `../../web/dist` |
| devUrl | `http://127.0.0.1:5173` |
| window | label `main`, 1080×760, min 800×560, Overlay title bar, hidden title, not created by Tauri |
| withGlobalTauri | `false` |
| CSP | `default-src 'self'`; `connect-src 'self' ipc: http://ipc.localhost`; no remote origins |
| bundle targets | `["app"]` in config; the release workflow overrides with `--bundles app,dmg` / `deb,appimage` / `nsis` |
| macOS signing | `"-"` (ad-hoc) |

`Info.plist` adds `NSLocalNetworkUsageDescription`, because macOS treats a
connection to a LAN host as a protected resource. Without it the system refuses
the dial (`EHOSTUNREACH`) and the tunnel manager looks broken on the networks it
exists for.

**Section sources**

- [apps/desktop/src-tauri/tauri.conf.json](file://apps/desktop/src-tauri/tauri.conf.json#L1-L60)
- [apps/desktop/src-tauri/Info.plist](file://apps/desktop/src-tauri/Info.plist#L1-L17)

## Build and e2e

`apps/desktop` is not a pnpm workspace dependency of `apps/web`; the coupling is
a path (`frontendDist: ../../web/dist`). Its `build` script:

- checks `apps/web/dist/index.html` and prints a non-fatal warning if missing;
- runs `cargo check --manifest-path src-tauri/Cargo.toml` with `.nothrow()`;
- writes `dist/shell.json` with `{webPresent, cargoOk, frontendDist, devUrl}`
  read from `tauri.conf.json`, so the manifest cannot disagree with the bundle.

`pnpm build:app` runs `tauri build`; that is what produces the real `.app` and
the installers in the release workflow.

The e2e suite uses WebdriverIO with `@wdio/tauri-service` and the app's embedded
WebDriver server. `SSH_TUNNEL_WEBDRIVER=1` gates the plugin in `lib.rs`, and the
config defaults that variable so a bare `wdio run` works. Specs assert:

- the window title contains "SSH Tunnel";
- `.tunnel-toolbar`, `.tunnel-sidebar`, `.tunnel-content` exist and are
  displayed;
- a "New tunnel" button exists;
- `document.documentElement.dataset.backdrop` is `glass` or `vibrancy`, and
  `--material-opacity` responds to a style change;
- a screenshot is written to `screenshots/desktop-app.png`.

The capabilities path is hard-coded to
`target/release/bundle/macos/SSH Tunnel.app/...`, so the suite is macOS-only as
written, even though the shell compiles on three OSes.

**Section sources**

- [apps/desktop/scripts/build.ts](file://apps/desktop/scripts/build.ts#L1-L60)
- [apps/desktop/e2e/wdio.conf.ts](file://apps/desktop/e2e/wdio.conf.ts#L1-L60)
- [apps/desktop/e2e/specs/app.js](file://apps/desktop/e2e/specs/app.js#L1-L75)
- [apps/desktop/package.json](file://apps/desktop/package.json#L1-L30)

## Gaps and Drift

1. **Stale descriptions.** `package.json` says the shell "spawns the daemon (or
   attaches to one)" and `Cargo.toml` says "spawns the tunnel daemon sidecar";
   both describe the pre-`85d04e0` design. `src/index.ts` still calls itself a
   stub.
2. **`ProfileInput` is duplicated.** A new profile field must be added to
   `ssh-contract`, `crates/tunnel-core/src/contract.rs`, and
   `tunnel_commands.rs`.
3. **Close-session asymmetry.** IPC returns `Ok(bool)`; HTTP returns `404` when
   no session exists. The client discards both bodies.
4. **One mutex for all commands.** Long starts/stops serialize polling; there is
   no per-profile concurrency.
5. **Health version source differs.** `CARGO_PKG_VERSION` here,
   `SERVICE_VERSION` in the daemon.
6. **e2e is macOS-only** and not part of CI; the release job builds but never
   runs the WebDriver specs.

**Section sources**

- [apps/desktop/package.json](file://apps/desktop/package.json#L1-L10)
- [apps/desktop/src-tauri/Cargo.toml](file://apps/desktop/src-tauri/Cargo.toml#L8-L14)
- [apps/desktop/src-tauri/src/tunnel_commands.rs](file://apps/desktop/src-tauri/src/tunnel_commands.rs#L96-L155)
- [.github/workflows/ci.yml](file://.github/workflows/ci.yml#L86-L130)
