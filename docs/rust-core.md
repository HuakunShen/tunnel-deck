# Design: a Rust core, and no JavaScript runtime in the installed app

Status: **Phase 1 implemented** (app embeds `tunnel-core` over Tauri IPC; no
Node in the bundle). Phases 2–3 (`ssh-tunnel serve`/CLI, Kunkun extension)
remain future work. Supersedes the "one backend, many thin fronts" section of
`docs/design.md` only where noted below.

## Requirement

The product must work when someone downloads the `.dmg` / `.exe` / `.deb` and
opens it. That person is not expected to install Node, Deno, Bun, or anything
else: **the app may not carry or depend on a JavaScript runtime**.

Today it does. `apps/desktop` spawns `node server.mjs`, and before that it
probes `PATH`, the login shell, `Program Files`, Volta and nvm-windows to find
an interpreter it can borrow. Measured on this machine:

| | Node daemon (v0.1.3) |
|---|---|
| resident memory | **72.7 MB** |
| startup | ~120 ms |
| failure modes | missing `node`, PATH mismatch, stale `daemon.json`, port 7432 taken |

The CLI is a different audience: `npx ssh-tunnel` is run by people who already
have Node. It may keep using it.

The same code is also going to become a Kunkun extension
(`~/ExtDev/kunkun-v2.1`), where a JavaScript backend is the thing we most want
to avoid: extensions get a sandboxed worker by default, and a sidecar costs
memory for as long as it runs.

## What Kunkun allows (read from the v2.1 source)

- **Frontend-only commands** (view / no-view / frame) run in a sandboxed
  worker. They cannot spawn arbitrary programs: the `shell` permission is
  validated against a pinned table of 26 system programs
  (`packages/app-protocol/src/shell-policy.ts` → `LEGACY_SHELL_PROGRAMS`,
  enforced by `hasClosedAuthority`). There is no way to declare
  "run my own binary" through `shell`.
- **`binaries`** (`{name, installHint}`) only *resolves* a program on `PATH`
  for display; the docs are explicit that a lookup "runs nothing and grants no
  execution authority".
- **Sidecars** are the supported way to run our own code, and `runtime` may be
  `"binary"`: the entry is a file inside the extension, `chmod 755` at
  install, pruned per platform through `kunkun.platforms.targets`. The host
  passes the port in an env var (`port.env`), waits on `ready.http`, can put a
  secret on every request (`auth: "kunkun"`), and the view reaches the sidecar
  through its own content origin (`/ext/<id>/sidecar/<name>/…`), so the page
  never sees the secret.

Conclusion: a Kunkun extension for this product is a **view + one binary
sidecar**, and that binary is the same core the desktop app runs. No Node, no
worker backend.

## Decision

Write the tunnel core once, in Rust, and embed it three ways:

```
                      crates/tunnel-core  (no UI, no HTTP, no Tauri)
                       ├─ profiles: read/validate/write profiles.json
                       ├─ argv:     ssh argv exactly as today
                       ├─ spawn:    detached ssh, per-profile log
                       ├─ control:  ControlMaster check/exit (POSIX)
                       ├─ probe:    TCP dial per forward
                       ├─ runtime:  runtime.json, adoption by socket or pid
                       ├─ logs:     tail from the end of a log file
                       └─ sessions: terminal handoff + ssh-config inventory
                                  │
        ┌─────────────────────────┼──────────────────────────────┐
        │                         │                              │
  apps/desktop (Tauri)     crates/tunnel-cli               crates/tunnel-http
  links the crate,         `ssh-tunnel list|start|…`       `ssh-tunnel serve`
  #[tauri::command]        (--json; what Kunkun calls       (same routes as
  no sidecar, no port      indirectly through its own       today's daemon)
                            binary sidecar)
```

- **Desktop app**: in-process. No sidecar, no port, no `daemon.json`, no
  interpreter probe. The window is the only process the app owns besides the
  `ssh` children it deliberately leaves detached.
- **Kunkun extension**: view + `ssh-tunnel serve` as a `runtime: "binary"`
  sidecar. One ~5 MB process while the extension is open, killed with the
  host (`KUNKUN_PARENT_PID`).
- **CLI** (`crates/tunnel-cli`): no daemon needed for a single action; reads
  and writes the same files. This is what makes the app's state visible
  outside the app without a server.
- **Other fronts** (TUI, VS Code, DSH, browser): unchanged, they speak HTTP to
  `ssh-tunnel serve`.

### What does not change

- **File formats**: `~/.ssh-tunnel-ui/{profiles.json, runtime.json, logs/,
  control/}` stay byte-compatible. A v0.1.3 install keeps its tunnels; a Node
  front and a Rust front can adopt each other's tunnels because ownership is
  decided by the ControlMaster socket (POSIX) or the recorded pid (Windows),
  not by who wrote the file.
- **The wire contract**: the JSON shapes in `packages/ssh-contract` stay the
  source of truth. `tunnel-http` answers the same routes; the TUI, VS Code
  extension and DSH plugin need no change.
- **The ssh command**: `buildSshArgv` is ported branch for branch, and both
  implementations assert against the same fixtures
  (`fixtures/argv/*.json`), so "the command the UI shows" and "the command
  that runs" cannot drift.

### UI transport

`packages/ssh-client` today is `fetch` + a base URL. It becomes an interface
with two implementations:

- `fetchTransport(baseUrl)` — browser, TUI, VS Code, DSH (unchanged behaviour).
- `invokeTransport(invoke)` — inside the Tauri window, one `invoke` per
  operation.

The Svelte app picks one at boot (`window.__TAURI__` / an injected flag), so
components keep talking to the same typed client. The CSP loses
`http://127.0.0.1:7432` in the app build, and `tauri dev` points `devUrl` at
the Vite dev server instead of the daemon.

### Why not the cheaper options

- **Bun single-file binary** (`bun build --compile`): works (89 MB), but it is
  a JavaScript runtime with a bundle, ~50 MB RSS, and it would still be
  spawned as a child process by the app. Against the requirement.
- **Node SEA / Deno compile**: same runtime, same size.
- **Keep the Node daemon and require Node**: rejected by the requirement.
- **Tauri commands + a second Rust daemon later**: that is what this design
  does, in the opposite order — the core crate is written first so both
  embeddings share it.

## Phasing

**Phase 1 — the app stops needing Node.**

1. `crates/tunnel-core` (~1 500–2 000 lines incl. tests): port of
   `packages/ssh-core` + `packages/ssh-host-node` + `packages/ssh-service`.
   Ports stay injectable (`Process`, `Probe`, `Clock`) so the state machine is
   testable without spawning anything.
2. `apps/desktop/src-tauri`: link the crate; `#[tauri::command]`s mirror the
   HTTP routes 1:1; delete `spawn_sidecar`, the node probe, `--web-dir`, and
   the daemon health wait.
3. `apps/web`: `invokeTransport` + boot switch; no other component changes.
4. Cross-implementation fixtures in CI: `cargo test` and `vitest` both assert
   the same argv and the same JSON shapes.

Phase 1 is a strict subset: the TUI, CLI, VS Code extension and DSH plugin keep
running against the existing Node daemon, unchanged, so the release can still
ship while the Rust path is validated.

**Phase 2 — one core, one binary.**

5. `crates/tunnel-http` (axum) exposing the same routes; `ssh-tunnel serve`.
6. `crates/tunnel-cli`: `list|add|show|preview|start|stop|probe|logs|serve`,
   `--json` everywhere, exit codes mapped from `TUNNEL_PROBLEM_CODES`.
7. The npm packages become thin clients: `apps/cli` keeps argument parsing but
   delegates to `ssh-tunnel serve` when it is running; `--direct` is retired
   once the TUI has a server to talk to (or the TUI spawns `ssh-tunnel serve`
   itself, the way the app does today).
8. Release pipeline builds the binary per platform (same matrix as now) and
   packages it into the app bundle as a resource.

**Phase 3 — Kunkun extension.**

9. `ssh-tunnel serve --port=$KUNKUN_SIDECAR_PORT`, `auth: "kunkun"`,
   `ready.http: "/api/health"`, `platforms.targets` per binary.
10. View: a Frame command with the same console (Svelte components extracted
    from `apps/web` where practical).

## Risks and open questions

- **Two implementations during Phase 1/2.** Mitigated by shared fixtures and
  by deleting the TypeScript manager in Phase 2 rather than growing both.
- **Terminal handoff in Rust**: AppleScript/`open -a` on macOS, `gnome-terminal`
  family on Linux, `wt.exe`/`powershell` on Windows — a straight port, but the
  settle-window behaviour (`TERMINAL_START_GRACE_MS`) has to be kept.
- **Kunkun per-platform binary packaging**: `sidecars[].entry` is a single path,
  so the archive has to place the right binary there at pack time and let
  `platforms.targets` prune the rest. Needs a spike against `kunkun-ext pack`.
- **Windows**: OpenSSH there has no `ControlMaster`; ownership is by pid (already
  implemented in `packages/ssh-service/src/manager.ts` and ported as-is).
- **Binary size**: axum + tokio + rustls ≈ 6–9 MB per platform, versus 89 MB for
  a compiled Bun runtime. App bundle stays around the current 5 MB compressed.
