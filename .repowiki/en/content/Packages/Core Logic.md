# Core Logic

<cite>
**Referenced Files in This Document**
- [packages/ssh-core/package.json](file://packages/ssh-core/package.json)
- [packages/ssh-core/src/index.ts](file://packages/ssh-core/src/index.ts)
- [packages/ssh-core/src/argv.ts](file://packages/ssh-core/src/argv.ts)
- [packages/ssh-core/src/endpoint.ts](file://packages/ssh-core/src/endpoint.ts)
- [packages/ssh-core/src/validate.ts](file://packages/ssh-core/src/validate.ts)
- [packages/ssh-core/src/state-machine.ts](file://packages/ssh-core/src/state-machine.ts)
- [tests/core/argv.test.ts](file://tests/core/argv.test.ts)
- [tests/core/endpoint.test.ts](file://tests/core/endpoint.test.ts)
- [tests/core/validate.test.ts](file://tests/core/validate.test.ts)
- [tests/core/fixtures.test.ts](file://tests/core/fixtures.test.ts)
- [fixtures/argv/basic-local.json](file://fixtures/argv/basic-local.json)
</cite>

## Table of Contents

1. [Purpose](#purpose)
2. [Argv Building](#argv-building)
3. [Endpoint Derivation](#endpoint-derivation)
4. [Semantic Validation](#semantic-validation)
5. [State Machine](#state-machine)
6. [Purity and Boundaries](#purity-and-boundaries)
7. [Tests](#tests)

## Purpose

`@ssh-tunnel/core` holds the domain rules that do not touch the OS:
`buildSshArgv`, `localForwardEndpoint`/`profileEndpoints`, `validateProfileInput`,
`findPortConflicts`, and the lifecycle transition functions. It depends only on
`@ssh-tunnel/contract` and is one of the three packages guarded by
`check-boundaries`.

**Section sources**

- [packages/ssh-core/src/index.ts](file://packages/ssh-core/src/index.ts#L1-L10)
- [packages/ssh-core/package.json](file://packages/ssh-core/package.json#L1-L20)

## Argv Building

`buildSshArgv(profile, options)` returns `{ argv, command }`. `options` has two
fields:

- `controlPath?: string` — adds exactly
  `-o ControlMaster=yes -o ControlPath=… -o ControlPersist=no`;
- `interactive?: boolean` — drops `BatchMode=yes` and `-N` so the window gets a
  login shell.

The ordering (BatchMode, StrictHostKeyChecking, ExitOnForwardFailure,
ClearAllForwardings, keepalive, `-p`, `-i`, `-J`, forwards, extra `-o`,
control, destination, command/`-N`) is identical to Rust's `build_ssh_argv`, and
six JSON fixtures assert the shared result.

`forwardSpec` formats `[bind:]port:host:hostport` for local/remote and
`[bind:]port` for dynamic. `sshDestination` prefixes `sshUser@` and strips any
existing user prefix from `sshTarget` (Rust splits on the last `@`). An empty
identity file is ignored. `keepalive = serverAliveInterval ?? 30`, and a value
of 0 disables both keepalive options.

`quoteForDisplay` is display-only; it quotes only when a part is outside
`[A-Za-z0-9_@%+=:,./-]`.

**Section sources**

- [packages/ssh-core/src/argv.ts](file://packages/ssh-core/src/argv.ts#L1-L167)
- [fixtures/argv/basic-local.json](file://fixtures/argv/basic-local.json#L1-L15)

## Endpoint Derivation

`localForwardEndpoint(forward)` returns a browsable endpoint only for
`kind === "local"`:

- host = trimmed `bindAddress`, or `"localhost"` when absent/empty;
- `endpoint = "<host>:<bindPort>"`, `url = "http://<host>:<bindPort>"`.

Remote forwards bind on the far machine and dynamic forwards are SOCKS proxies,
so both return `undefined`. `profileEndpoints` filters a list;
`endpointSummary` joins the `host:port` strings with `", "`.

This is what the Web console uses for the "open" control: the UI never links to
a dead `localhost` for an `-R` or `-D` forward.

**Section sources**

- [packages/ssh-core/src/endpoint.ts](file://packages/ssh-core/src/endpoint.ts#L1-L69)

## Semantic Validation

`validateProfileInput(input)` returns `ProfileFinding[]` with `path`, `message`,
`severity` (`error` blocks start, `warning` is shown). The checks mirror Rust:

- blank `name` or `sshTarget`;
- zero forwards or more than 16;
- duplicate local/dynamic bind ports within one profile;
- remote forward bound to loopback (`""`, `localhost`, `127.0.0.1`, `::1`);
- local/dynamic port below 1024;
- `noRemoteCommand: false` with no `remoteCommand`;
- jump hops > 8, extra options > 32, notes > 2000.

`findPortConflicts(profiles)` returns `{ port, profileIds }` for ports claimed
by more than one profile.

The duplication with `ssh-contract` is intentional at different layers: zod
rejects malformed shapes, this function explains semantic problems with a field
path a form can highlight.

**Section sources**

- [packages/ssh-core/src/validate.ts](file://packages/ssh-core/src/validate.ts#L1-L142)

## State Machine

`TRANSITIONS` is a `Record<TunnelStatus, readonly TunnelStatus[]>`; `canTransition`
indexes it. `startTarget` returns `"starting"` from `stopped`/`failed`, else
`null`. `stopTarget` returns `"stopping"` from `starting`/`running`/`degraded`,
else `null`. `isStartable`/`isStoppable` wrap those.

Because the manager calls these helpers instead of re-deriving rules, an
illegal request becomes a `Conflict` with the same semantics in the CLI, TUI,
Web, DSH and VS Code.

**Section sources**

- [packages/ssh-core/src/state-machine.ts](file://packages/ssh-core/src/state-machine.ts#L1-L50)

## Purity and Boundaries

- No imports from `node:*`, `ssh-host-node` or `ssh-service`.
- No DOM globals.
- No I/O: argv and validation are pure; the state machine is pure data.
- `check:boundaries` scans this package for all of those violations and for
  relative imports that escape `src`.

**Section sources**

- [scripts/check-boundaries.ts](file://scripts/check-boundaries.ts#L12-L20)

## Tests

| Suite | Coverage |
|---|---|
| `tests/core/argv.test.ts` | every flag, ordering-sensitive options, control trio, interaction of `noRemoteCommand` and `remoteCommand` |
| `tests/core/endpoint.test.ts` | local-only endpoints, bind-address handling, summary text |
| `tests/core/validate.test.ts` | semantic findings, severities, cross-profile port conflicts, state transitions |
| `tests/core/fixtures.test.ts` | parses `fixtures/argv/*.json` and `fixtures/state-machine.json`, asserts `mustContain`/`mustNotContain` and every allowed/rejected transition |

The fixture suite is the cross-language gate: the same JSON is read by
`crates/tunnel-core/tests/fixtures.rs`.

**Section sources**

- [tests/core/fixtures.test.ts](file://tests/core/fixtures.test.ts#L1-L55)
- [tests/core/argv.test.ts](file://tests/core/argv.test.ts#L1-L30)
