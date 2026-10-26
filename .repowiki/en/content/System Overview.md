# System Overview

<cite>
**Referenced Files in This Document**
- [README.md](file://README.md)
- [package.json](file://package.json)
- [pnpm-workspace.yaml](file://pnpm-workspace.yaml)
- [Cargo.toml](file://Cargo.toml)
- [docs/design.md](file://docs/design.md)
- [docs/rust-core.md](file://docs/rust-core.md)
- [crates/tunnel-core/src/lib.rs](file://crates/tunnel-core/src/lib.rs)
- [packages/ssh-contract/src/index.ts](file://packages/ssh-contract/src/index.ts)
- [packages/ssh-service/src/serve.ts](file://packages/ssh-service/src/serve.ts)
- [apps/desktop/src-tauri/src/lib.rs](file://apps/desktop/src-tauri/src/lib.rs)
- [apps/web/src/lib/store.ts](file://apps/web/src/lib/store.ts)
- [fixtures/state-machine.json](file://fixtures/state-machine.json)
- [.github/workflows/ci.yml](file://.github/workflows/ci.yml)
</cite>

## Table of Contents

1. [Purpose](#purpose)
2. [Two Implementations of One Domain](#two-implementations-of-one-domain)
3. [Runtime Topology](#runtime-topology)
4. [Fronts and Transports](#fronts-and-transports)
5. [Persistent State](#persistent-state)
6. [Lifecycle and Error Model](#lifecycle-and-error-model)
7. [Tunnel Ownership](#tunnel-ownership)
8. [What Is Actually Implemented](#what-is-actually-implemented)
9. [Document Provenance](#document-provenance)

## Purpose

`ssh-tunnel-ui` is a manager for SSH tunnels, not an SSH implementation. The
data plane stays in the system `ssh` binary (`ssh -N` with `-L`/`-R`/`-D`/`-J`);
the management plane remembers profile definitions, spawns and supervises the
`ssh` child, probes forwarded ports, persists JSON, and serves several fronts.

The repository states that non-goal directly: "Reimplementing the SSH data
plane (stays in system OpenSSH)" and "Storing secrets" (`docs/design.md`).

**Section sources**

- [docs/design.md](file://docs/design.md#L1-L20)
- [README.md](file://README.md#L1-L6)

## Two Implementations of One Domain

The product now contains two independently executable implementations of the
same tunnel domain. This is the single most important fact about the codebase.

| Concern | TypeScript track | Rust track |
|---|---|---|
| Wire contract | `packages/ssh-contract` (zod) | `crates/tunnel-core/src/contract.rs` (serde) |
| Domain rules | `packages/ssh-core` | `argv.rs`, `validate.rs`, `state_machine.rs` |
| Host syscalls | `packages/ssh-host-node` | `host.rs` + `ports.rs` traits |
| Manager | `packages/ssh-service/src/manager.ts` | `crates/tunnel-core/src/manager.rs` |
| Transport | Hono HTTP on loopback | Tauri IPC commands |
| Consumers | server, CLI, TUI, Web, DSH, VS Code | Desktop app (`apps/desktop/src-tauri`) |
| Gate keeping them aligned | `fixtures/argv/*.json`, `fixtures/state-machine.json` | same fixtures via `crates/tunnel-core/tests/fixtures.rs` |

The Rust crate is deliberately host-free: "It mirrors `packages/ssh-core` +
`packages/ssh-contract`; the JSON wire shapes are identical, and `fixtures/`
adjudicates both implementations" (`crates/tunnel-core/src/lib.rs`).

**Section sources**

- [crates/tunnel-core/src/lib.rs](file://crates/tunnel-core/src/lib.rs#L1-L18)
- [packages/ssh-contract/src/index.ts](file://packages/ssh-contract/src/index.ts#L1-L11)
- [fixtures/state-machine.json](file://fixtures/state-machine.json#L1-L6)
- [docs/rust-core.md](file://docs/rust-core.md#L1-L16)

## Runtime Topology

```text
                        system `ssh` processes (data plane, detached)
                          ^                    ^
             spawn/probe  |                    |  ControlMaster socket
                          |                    |  (`ssh -O check|exit`)
        +-----------------+----+        +------+------------------+
        | Node host ports      |        | Rust host implementations|
        | @ssh-tunnel/host-node|        | tunnel-core::host        |
        +----------+-----------+        +------------+-------------+
                   |                                 |
        +----------v-----------+          +----------v-----------+
        | TunnelManager (TS)   |          | Manager<P,Q,S,...>   |
        | @ssh-tunnel/service  |          | tunnel-core          |
        +----------+-----------+          +----------+-----------+
                   |                                 |
        +----------v-----------+          +----------v-----------+
        | Hono API + static UI |          | Tauri #[command]s    |
        | 127.0.0.1:7432       |          | in-process, no port  |
        +----------+-----------+          +----------+-----------+
                   |                                 |
   server / CLI / TUI / Web / DSH / VS Code     Desktop WebView (apps/web)
```

There is no process boundary inside the desktop app: the shell links
`tunnel-core` and registers typed commands, and the same Svelte console calls
them through `createInvokeTunnelClient`. The HTTP daemon and the desktop app
write the same files under `~/.ssh-tunnel-ui`, which is why they can adopt each
other's tunnels.

**Section sources**

- [apps/desktop/src-tauri/src/lib.rs](file://apps/desktop/src-tauri/src/lib.rs#L1-L72)
- [apps/desktop/src-tauri/src/tunnel_commands.rs](file://apps/desktop/src-tauri/src/tunnel_commands.rs#L1-L71)
- [packages/ssh-service/src/serve.ts](file://packages/ssh-service/src/serve.ts#L1-L108)
- [docs/design.md](file://docs/design.md#L21-L52)

## Fronts and Transports

| Front | Package | Transport | Manager location |
|---|---|---|---|
| Daemon | `apps/server` | HTTP server | in daemon process |
| CLI | `apps/cli` | HTTP (default) or in-process (`--direct`) | daemon or CLI process |
| TUI | `apps/tui` | HTTP (default) or in-process (`--direct`) | daemon or TUI process |
| Web console | `apps/web` | HTTP (same origin / `?daemon=` override), Tauri IPC inside the app | daemon or Tauri shell |
| Desktop | `apps/desktop` | Tauri IPC only | Tauri process |
| DSH panel | `integrations/dsh` | its own in-process Hono app mounted on the Harness server | Harness process |
| VS Code | `integrations/vscode` | HTTP to an attached or spawned daemon | spawned/attached daemon |
| GPUI window | `apps/gpui` (untracked) | HTTP to a daemon | daemon |

The Web console chooses its transport at boot: `window.__TAURI_INTERNALS__`
selects IPC; `vscode-webview:` forces `http://127.0.0.1:7432`; anything else is
same-origin HTTP (`apps/web/src/lib/store.ts`).

**Section sources**

- [apps/web/src/lib/store.ts](file://apps/web/src/lib/store.ts#L36-L82)
- [README.md](file://README.md#L63-L78)
- [integrations/dsh/src/host.ts](file://integrations/dsh/src/host.ts#L1-L40)
- [integrations/vscode/src/supervisor.ts](file://integrations/vscode/src/supervisor.ts#L1-L20)

## Persistent State

All managers share one directory layout, `~/.ssh-tunnel-ui` (overridable in
parts):

| Path | Contents | Failure behaviour |
|---|---|---|
| `profiles.json` | array of `TunnelProfile` | Rust corrupt-read degrades to empty; Node store raises `StoreFailed` |
| `runtime.json` | `RuntimeRecord[]` of what was up | best-effort; unreadable/corrupt is treated as "nothing was running" |
| `logs/<profileId>.log` | detached ssh stdout+stderr | appended, never rotated; tail reads at most 128 KiB |
| `control/<profileId>.sock` | per-profile ControlMaster socket (POSIX) | stale files reclaimed before start/handoff |
| `daemon.json` | `{port, host, instanceId, version, pid}` discovery file | best-effort, used by CLI/TUI |

The profile store and the runtime store are intentionally different contracts:
profiles are user configuration and must be validated; runtime records are a
disposable claim that must never block a boot (`packages/ssh-host-node/src/runtime-store.ts`).

**Section sources**

- [packages/ssh-host-node/src/store.ts](file://packages/ssh-host-node/src/store.ts#L1-L69)
- [packages/ssh-host-node/src/runtime-store.ts](file://packages/ssh-host-node/src/runtime-store.ts#L1-L92)
- [packages/ssh-service/src/serve.ts](file://packages/ssh-service/src/serve.ts#L40-L100)
- [crates/tunnel-core/src/host.rs](file://crates/tunnel-core/src/host.rs#L196-L264)

## Lifecycle and Error Model

Statuses are `stopped → starting → running ⇄ degraded → stopping → stopped`,
with `failed` re-startable. The transition table is data in both languages and
the same `fixtures/state-machine.json` asserts every allowed and rejected pair.

Failures are a closed set of 14 codes (`TUNNEL_PROBLEM_CODES` in TypeScript,
`TunnelProblemCode` in Rust). One code maps to exactly one HTTP status through
an exhaustive record; clients branch on `code`, never on message text. Tauri
returns the same `{ code, message, retryable }` shape as the HTTP problem
envelope, so the shared client validates both identically.

**Section sources**

- [packages/ssh-core/src/state-machine.ts](file://packages/ssh-core/src/state-machine.ts#L1-L50)
- [crates/tunnel-core/src/state_machine.rs](file://crates/tunnel-core/src/state_machine.rs#L1-L60)
- [packages/ssh-contract/src/errors.ts](file://packages/ssh-contract/src/errors.ts#L1-L60)
- [crates/tunnel-core/src/error.rs](file://crates/tunnel-core/src/error.rs#L1-L60)

## Tunnel Ownership

A tunnel can be `managed` (this process spawned it), `terminal` (the same
command runs in a terminal window this host launched, reachable through its
ControlMaster socket), or `external` (inferred from a listening bind port).
This distinction is what lets the UI say something honest after a restart:

- POSIX: every managed/handed-off tunnel carries `ControlMaster=yes`,
  `ControlPath=…/control/<id>.sock`, `ControlPersist=no`. Liveness is
  `ssh -O check`; stopping is `ssh -O exit`.
- Windows: OpenSSH has no `ControlMaster`, so ownership falls back to the
  recorded pid and `tasklist`/`taskkill`.

**Section sources**

- [packages/ssh-contract/src/tunnel.ts](file://packages/ssh-contract/src/tunnel.ts#L118-L145)
- [packages/ssh-host-node/src/control.ts](file://packages/ssh-host-node/src/control.ts#L1-L79)
- [crates/tunnel-core/src/host.rs](file://crates/tunnel-core/src/host.rs#L266-L331)
- [crates/tunnel-core/src/manager.rs](file://crates/tunnel-core/src/manager.rs#L278-L337)

## What Is Actually Implemented

Implemented and exercised in the working tree:

- TypeScript daemon, CLI, TUI, static Web console, Tauri desktop app, DSH plugin,
  VS Code extension.
- Rust `tunnel-core` with fixtures, unit tests, Tauri IPC adapter.
- CI for typecheck, boundaries, tests, Rust tests/clippy, and a three-OS shell
  `cargo check`; a tag-driven release workflow for macOS/Windows/Linux bundles.

Described in `docs/rust-core.md` but **not present as code**: a `tunnel-http`
crate (axum), a `tunnel-cli` crate, a standalone `ssh-tunnel serve` binary, and
the Kunkun extension integration. Those are Phase 2–3 proposals.

Present on disk but **not committed**: `apps/gpui` (a GPUI experiment with
`tunnel-api` and `tunnel-console`). `git status` reports it as untracked, so it
is not part of any build, CI job, or release in this repository's history.

**Section sources**

- [docs/rust-core.md](file://docs/rust-core.md#L130-L200)
- [.github/workflows/ci.yml](file://.github/workflows/ci.yml#L1-L100)
- [Cargo.toml](file://Cargo.toml#L1-L22)

## Document Provenance

This wiki was generated from the working tree and read-only git history on
2026-10-25, at HEAD `0675e0e` ("feat(web): show starting and stopping feedback
while actions run"). The repository has no `.journal/` or `.reports/`
directories, so no journal or experiment sources exist to cite; every claim
below is derived from source files, configuration, workflows, and the 49
commits from `d392801` (2026-10-11) to `0675e0e`.

**Section sources**

- [README.md](file://README.md#L166-L183)
- [.gitmodules](file://.gitmodules#L1-L3)
