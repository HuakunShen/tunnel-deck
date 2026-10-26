# CLI

<cite>
**Referenced Files in This Document**
- [apps/cli/package.json](file://apps/cli/package.json)
- [apps/cli/src/cli.ts](file://apps/cli/src/cli.ts)
- [apps/cli/scripts/build.ts](file://apps/cli/scripts/build.ts)
- [packages/ssh-client/src/index.ts](file://packages/ssh-client/src/index.ts)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts)
- [packages/ssh-contract/src/tunnel.ts](file://packages/ssh-contract/src/tunnel.ts)
- [README.md](file://README.md)
</cite>

## Table of Contents

1. [Entry and Discovery](#entry-and-discovery)
2. [Two Modes](#two-modes)
3. [Commands](#commands)
4. [Output and Errors](#output-and-errors)
5. [Direct-Mode Limitations](#direct-mode-limitations)
6. [Help Text Drift](#help-text-drift)
7. [Gaps](#gaps)

## Entry and Discovery

`apps/cli` publishes the binary `ssh-tunnel` → `dist/cli.mjs`, bundled by
esbuild with no externals. It depends on the client, contract, host-node and
service packages.

Daemon URL resolution, in order:

1. `--daemon=http://host:port` (trailing slash stripped);
2. `~/.ssh-tunnel-ui/daemon.json` `{host, port}`;
3. `http://127.0.0.1:7432`.

The discovery file is written by `startDaemon` and is best-effort; a missing or
corrupt file silently falls back to the default, so a daemon on a non-default
port is unreachable unless `--daemon=` is passed.

**Section sources**

- [apps/cli/src/cli.ts](file://apps/cli/src/cli.ts#L17-L36)
- [apps/cli/package.json](file://apps/cli/package.json#L1-L25)
- [packages/ssh-service/src/serve.ts](file://packages/ssh-service/src/serve.ts#L100-L108)

## Two Modes

- **Daemon mode (default)**: `createTunnelClient({ baseUrl: daemonBase(), fetch })`.
- **Direct mode (`--direct`)**: constructs a `TunnelManager` in the CLI process
  with `createNodeProcessPort`, `createNodeProbePort`,
  `createFileStorePort(<base>/profiles.json)`, `systemClock`, and
  `<base>/logs`.

The two modes expose the same command set, but not the same capabilities. See
[Direct-Mode Limitations](#direct-mode-limitations).

**Section sources**

- [apps/cli/src/cli.ts](file://apps/cli/src/cli.ts#L38-L55)
- [apps/cli/src/cli.ts](file://apps/cli/src/cli.ts#L92-L98)

## Commands

| Command | Behaviour |
|---|---|
| `list` | tab-separated `id`, status dot+word, name, ssh target |
| `add --file=<json>` | validates with `profileInputSchema`, creates, prints id + name |
| `show <id>` | prints the profile JSON |
| `preview <id>` | prints the exact `ssh …` display command |
| `logs <id> [--tail=N]` | path + byte count on stderr, lines on stdout (clamped 1..2000) |
| `start <id>` | prints `<id>: <status>`, plus `note:` when `lastError` is set |
| `stop <id>` | prints `<id>: <status>` |
| `stop-all` | prints the number of stopped tunnels |
| `probe <id>` | one line per forward: kind, port, ok/FAIL, latency, detail |
| `delete <id>` | deletes and prints `deleted <id>` |

The status glyphs are `●` running, `◐` degraded, `✖` failed, `○` otherwise.
`add` also accepts the file path as a trailing positional argument.

**Section sources**

- [apps/cli/src/cli.ts](file://apps/cli/src/cli.ts#L68-L215)

## Output and Errors

- Success output is human text on stdout; the log path goes to stderr so
  `ssh-tunnel logs id | less` works.
- Failures print `error: <message>` to stderr and set `process.exitCode = 1`;
  the CLI does not map `TunnelError.code` to distinct exit codes.
- `start` deliberately prints `lastError` as a note because a failed start
  returns a state object, not a thrown error.
- Unknown commands print `unknown command <name>; see ssh-tunnel help` and exit
  non-zero.

**Section sources**

- [apps/cli/src/cli.ts](file://apps/cli/src/cli.ts#L196-L215)

## Direct-Mode Limitations

The direct manager receives only five options, so it has:

- no `terminal` port → terminal detection returns `[]`, and `launchInTerminal`
  is unavailable (the CLI has no launch command anyway);
- no `control` port → `multiplexed = false`, so no ControlMaster options are
  added and ownership falls back to pids;
- no `runtime` store → no adoption after a restart;
- no `openUrl` port → `openUrl` would be `UnsupportedOperation`.

Consequently a tunnel started by the daemon (which uses a control socket) is
not adoptable by `--direct`, and a `--direct` start is not adoptable by the
daemon on POSIX (no socket), only inferable from a listening bind port. This is
inherent in the capability model, not a bug, but it should be documented for
users who mix modes.

**Section sources**

- [apps/cli/src/cli.ts](file://apps/cli/src/cli.ts#L38-L55)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L139-L153)
- [packages/ssh-service/src/manager.ts](file://packages/ssh-service/src/manager.ts#L429-L476)

## Help Text Drift

The help block advertises `hosts` but the `switch` has no `hosts` case, so
`ssh-tunnel hosts` falls through to "unknown command". Conversely, `logs` is
implemented but missing from the help block. `ssh-tunnel help` is also not a
listed usage line beyond `help`/`--help`.

This is user-visible and trivially fixable; it is recorded rather than silently
documented as working.

**Section sources**

- [apps/cli/src/cli.ts](file://apps/cli/src/cli.ts#L68-L90)
- [apps/cli/src/cli.ts](file://apps/cli/src/cli.ts#L150-L215)

## Gaps

1. **No `--json` output**, despite `docs/rust-core.md` planning it for a future
   Rust CLI. Machine consumption requires parsing human text.
2. **No `hosts` command** even though the daemon exposes `GET /api/ssh-hosts`.
3. **No `launch`/`open-url` commands**, so the CLI cannot use the terminal
   handoff or browser opening.
4. **Exit codes are all 1**, so scripts cannot distinguish `NotFound` from
   `Conflict` from a transport failure.
5. **No `--web-dir`/serve command**; the CLI is a client only.
6. **`add` has no interactive form**; a JSON file is required.

**Section sources**

- [apps/cli/src/cli.ts](file://apps/cli/src/cli.ts#L1-L20)
- [docs/rust-core.md](file://docs/rust-core.md#L150-L175)
- [README.md](file://README.md#L126-L150)
