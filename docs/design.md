# Design: TunnelDeck

## Goal

Remember every SSH tunnel; one click applies it, one click stops it. The
`ssh -L/-R/-D/-J` flags are hard to memorize — this manager stores profiles
and shows the exact command it runs, so users learn by seeing.

## Non-goals

- Reimplementing the SSH data plane (stays in system OpenSSH).
- Storing secrets: keys/agents come from the user's own SSH setup; the store
  holds only public profile fields (targets, ports, options).
- Exposing tunnels beyond loopback by default (bind defaults to loopback).

## Architecture: one backend, many thin fronts

```
                +-------------------+
                |  system `ssh -N`   |  data plane (untouched)
                +--------+----------+
                         | spawn / TCP probe / ~/.ssh/config / JSON store
              +----------v-----------+
              | @ssh-tunnel/host-node|  trusted Node host (ports)
              +----------+-----------+
                         |
              +----------v-----------+
              | @ssh-tunnel/service  |  TunnelManager + Hono HTTP API
              +----------+-----------+
                         |
     +---------+---------+---------+---------+
     |         |         |         |         |
   server     CLI       TUI       Web     desktop/DSH/VSCode
  (daemon)  (dual)   (OpenTUI)  (static)   (stubs, same API)
```

Portable packages (`contract`, `core`, `client`) are host-free by
construction; `scripts/check-boundaries.ts` enforces it at `file:line:column`
granularity. Internal packages export TS source directly; only runnable
artifacts (server/cli/tui bundles) go through esbuild.

## Tunnel kinds (all verified live)

| kind | flags | test |
|---|---|---|
| local | `-L [bind:]port:host:hostport` | ufo:22 via 18081, `hostname`=ufo |
| remote | `-R [bind:]port:host:hostport` | ufo:19081 → local echo, `ECHO:hello-remote` |
| dynamic | `-D [bind:]port` | mini:11081, SOCKS5 handshake → ufo banner |
| jump | `-J hop…` + any forward | via ufo → mini, `hostname`=mini |
| multi | several `-L/-R/-D` in one process | unit-tested (argv has all three) |
| windows target | `-L` to OpenSSH_for_Windows | banner `SSH-2.0-OpenSSH_for_Windows_9.5` |

Cross-cutting: `ExitOnForwardFailure` (default on), keepalive defaults,
identity file, `-p`, `-o` passthrough, `-N`/remote command, stop-all,
per-forward TCP probes → `running`/`degraded`.

## Lifecycle

`stopped → starting → running ⇄ degraded → stopping → stopped`,
`failed` re-startable. `startTarget`/`stopTarget` in core decide; the HTTP
layer maps `Conflict`→409 etc. via an exhaustive code→status record. Startup
grace scales with jump hops (`900ms + 4s/hop`); probes retry 3× so slow
listeners don't false-degrade.

## Error model

Closed `TUNNEL_PROBLEM_CODES` (14) + `TunnelError(code)`; clients branch on
`code`, never message. `details` are flat redacted facts (ports, exit codes).

## Fronts share one contract

`packages/ssh-contract` (zod, strict requests / loose responses) → typed
`ssh-client` (injected `fetch`) → TUI, Web (static, zero-dep), CLI
(daemon + `--direct`), Tauri desktop shell, DSH plugin, VS Code stub. Adding
a front = new `apps/` or `integrations/` dir, no domain logic copied.

## Desktop shell (Tauri, real)

`apps/desktop/src-tauri` is a separate cargo workspace (structural isolation:
root builds never see WebView deps). The Rust shell is thin by design — it
links `crates/tunnel-core` in-process and exposes it through typed `tunnel_*`
commands; the Web UI talks to them through `createInvokeTunnelClient`. There
is no sidecar, no loopback port, and no JavaScript runtime to install.
`cargo check` + `cargo clippy` + `cargo test` + `tauri build` all pass; see
`docs/rust-core.md` for the core design and `fixtures/` for the cross-language
contract gates.

## DSH plugin (real host + client halves)

Follows the Refyard/Space Lens shape: `integrations/dsh/src/host.ts`
assembles a real `TunnelManager` + Hono app in the Harness process and mounts
it at `/ssh-tunnel` on the Harness webServer (loopback Host/Origin/
Sec-Fetch-Site guards, `SSH_TUNNEL_HOME`-overridable store);
`src/client.js` registers the panel and frames the workbench. Covered by
`tests/contract/dsh.test.ts` (mount, context, CRUD proxy, 403).

## VS Code extension (real)

`integrations/vscode` ships a real extension (space-lens shape): `supervisor.ts`
attaches to a healthy daemon or spawns the staged `server.mjs` (`fetch`
injectable for tests; spawn `error` swallowed, readiness timeout reports);
`tree.ts` renders the sidebar with running/stopped context values;
`extension.ts` wires start/stop/probe/stop-all/refresh/openWorkbench (webview
frames the daemon Web UI). esbuild CJS bundle, `vscode` external, vsix via
`vsce`. Covered by `tests/contract/vscode.test.ts` (attach, spawn failure,
tree render, offline clear).

## Live-test evidence (2026-10-11, this repo)

- daemon `health ok`, Web `200`, `ssh-hosts` lists ufo/mac-mini/rog16-windows.
- 21 tests green (16 core + 5 HTTP); 12 packages `tsc` clean; boundaries clean.
- stop-all frees all ports; store persists 5 profiles across daemon restarts.
