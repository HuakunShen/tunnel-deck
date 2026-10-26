# Contract

<cite>
**Referenced Files in This Document**
- [packages/ssh-contract/package.json](file://packages/ssh-contract/package.json)
- [packages/ssh-contract/src/index.ts](file://packages/ssh-contract/src/index.ts)
- [packages/ssh-contract/src/errors.ts](file://packages/ssh-contract/src/errors.ts)
- [packages/ssh-contract/src/tunnel.ts](file://packages/ssh-contract/src/tunnel.ts)
- [packages/ssh-contract/src/routes.ts](file://packages/ssh-contract/src/routes.ts)
- [crates/tunnel-core/src/contract.rs](file://crates/tunnel-core/src/contract.rs)
- [crates/tunnel-core/src/error.rs](file://crates/tunnel-core/src/error.rs)
</cite>

## Table of Contents

1. [Purpose](#purpose)
2. [Error Model](#error-model)
3. [Profile Schemas](#profile-schemas)
4. [State and Launch Schemas](#state-and-launch-schemas)
5. [Response Envelopes](#response-envelopes)
6. [Route Table](#route-table)
7. [Format Guarantees](#format-guarantees)
8. [Limits](#limits)

## Purpose

`@ssh-tunnel/contract` is the single source of truth for the wire. It imports
only `zod`, so it is safe in a browser, a Worker, an editor extension and Node.
Every other package infers its types from these schemas (`z.infer`), and the
Rust `contract.rs` mirrors the same field names and shapes.

The package exports three modules: errors, tunnel schemas, and the route table.

**Section sources**

- [packages/ssh-contract/src/index.ts](file://packages/ssh-contract/src/index.ts#L1-L11)
- [packages/ssh-contract/package.json](file://packages/ssh-contract/package.json#L1-L20)

## Error Model

`TUNNEL_PROBLEM_CODES` is a closed `as const` tuple of 14 codes:
`InvalidRequest`, `NotFound`, `Conflict`, `ResourceBusy`, `LimitExceeded`,
`SshNotFound`, `SshFailed`, `PortInUse`, `ProbeFailed`, `StoreFailed`,
`TerminalFailed`, `UnsupportedOperation`, `Timeout`, `InternalError`.

`TUNNEL_STATUS_BY_CODE: Record<TunnelProblemCode, number>` is exhaustive by
type: 400, 404, 409, 429, 429, 503, 502, 409, 502, 500, 502, 400, 504, 500.

`TunnelError` carries `code`, `message`, optional flat
`details: Record<string, string | number | boolean>`, and `retryable`. It
serializes through `toProblem()` for the HTTP envelope. `isTunnelProblem` is a
shape guard rather than `instanceof`, because problems cross serialization
boundaries (HTTP JSON, Tauri IPC).

The comment on `details` is a security rule: "Flat, redacted facts only: field
names, ports, exit codes. Never secrets."

**Section sources**

- [packages/ssh-contract/src/errors.ts](file://packages/ssh-contract/src/errors.ts#L1-L114)

## Profile Schemas

Forward rules are a discriminated union on `kind`:

```ts
local   { kind:"local",   bindAddress?, bindPort, targetHost, targetPort }
remote  { kind:"remote",  bindAddress?, bindPort, targetHost, targetPort }
dynamic { kind:"dynamic", bindAddress?, bindPort }
```

`bindAddress` is optional and max 255; an empty string means loopback in the
argv builders. Ports are integers 1..65535.

`TUNNEL_MODES = ["local", "remote", "dynamic", "jump", "multi"]` and
`tunnelModeSchema` is the enum. `mode` is descriptive; the argv builders do not
read it.

`tunnelProfileSchema` is a `strictObject` with:

- `id` 1..64, `name` 1..120, `mode`;
- `sshTarget` 1..512, `sshPort?`, `sshUser?`, `identityFile?`;
- `jumpHops?: string[]` max 8 items, each 1..512;
- `forwards` min 1, max 16;
- `extraOptions?: {key 1..128, value 1..1024}[]` max 32;
- `serverAliveInterval?` integer 0..3600;
- `exitOnForwardFailure?`, `clearAllForwardings?`, `noRemoteCommand?`,
  `remoteCommand?` (1..2048), `autoReconnect?`;
- `notes?` max 2000;
- `createdAt` 1..64, `updatedAt` 1..64.

`profileInputSchema` is the same schema with `id`, `createdAt`, `updatedAt`
omitted. Servers stamp those.

**Section sources**

- [packages/ssh-contract/src/tunnel.ts](file://packages/ssh-contract/src/tunnel.ts#L1-L130)

## State and Launch Schemas

- `TUNNEL_STATUSES`: `stopped`, `starting`, `running`, `degraded`, `failed`,
  `stopping`.
- `TUNNEL_ORIGINS`: `managed`, `terminal`, `external`.
- `LAUNCH_MODES`: `tunnel`, `shell`.
- `forwardHealthSchema`: `{ kind, bindPort, ok, latencyMs?, detail? }`.
- `tunnelStateSchema`: `{ profileId, status, origin?, pid?, startedAt?,
  lastError?, forwards, argv? }`.
- `sshHostInfoSchema`: `{ alias, hostName?, user?, port? }`.
- `terminalTargetSchema`: `{ id, label, hint, platform }`.
- `launchRequestSchema`: `{ terminalId, mode? }`; `launchResponseSchema` is the
  serialized `ExternalSession`.

The `pid` field is "positive" in zod and `u32` in Rust; an adopted session may
have no pid, which is why it is optional.

**Section sources**

- [packages/ssh-contract/src/tunnel.ts](file://packages/ssh-contract/src/tunnel.ts#L132-L270)
- [crates/tunnel-core/src/contract.rs](file://crates/tunnel-core/src/contract.rs#L100-L200)

## Response Envelopes

| Schema | Shape |
|---|---|
| `profileListResponseSchema` | `{ profiles: TunnelProfile[] }` |
| `stateListResponseSchema` | `{ states: TunnelState[] }` |
| `hostListResponseSchema` | `{ hosts: SshHostInfo[] }` |
| `terminalListResponseSchema` | `{ terminals: TerminalTarget[] }` |
| `logTailResponseSchema` | `{ profileId, path, lines[], truncated, bytes }` |
| `previewResponseSchema` | `{ profileId, argv[], command }` |
| `openUrlRequestSchema` | `{ url }` |
| `openUrlResponseSchema` | `{ url, openedAt }` |
| `healthResponseSchema` | `{ ok: true, version, serviceInstanceId }` |

`logTailResponseSchema` caps `lines` at 2000 entries of 4000 chars and carries
the absolute path so a front can show where the log lives without guessing.

**Section sources**

- [packages/ssh-contract/src/tunnel.ts](file://packages/ssh-contract/src/tunnel.ts#L200-L288)

## Route Table

`TUNNEL_ROUTES` declares 18 entries: health; profile list/create/get/update/
delete; preview; log; state list/get; start; stop; stop-all; probe; ssh hosts;
terminals; launch; open-url.

Helper builders (`profilePath`, `previewPath`, `logPath`, `statePath`,
`startPath`, `stopPath`, `probePath`, `launchPath`) URL-encode ids.

The table is documentation plus a naming convention, not the server's source of
truth: `http.ts` registers handlers literally, and the typed client uses
literals for several endpoints. The session-close route
(`DELETE /api/profiles/:id/session`) is absent from the table and has no helper.
This is recorded in [Boundaries and Drift](../Architecture/Boundaries%20and%20Drift.md).

**Section sources**

- [packages/ssh-contract/src/routes.ts](file://packages/ssh-contract/src/routes.ts#L1-L60)
- [packages/ssh-service/src/http.ts](file://packages/ssh-service/src/http.ts#L196-L216)

## Format Guarantees

- **Requests are strict**: `z.strictObject` rejects unknown keys, so a typo is a
  400 rather than a silent default.
- **Responses are forward-compatible**: clients parse with `.loose()`, so a
  newer daemon may add fields without breaking an older front.
- **Optionals are omitted, not null**: Rust uses `skip_serializing_if` and the
  client's `.loose()` readers accept absence.
- **No `enum`**: every closed set is an `as const` tuple with the type derived
  via `typeof`.
- **Version**: `CONTRACT_VERSION = "1.0.0"`.

**Section sources**

- [packages/ssh-contract/src/tunnel.ts](file://packages/ssh-contract/src/tunnel.ts#L280-L288)
- [packages/ssh-contract/src/index.ts](file://packages/ssh-contract/src/index.ts#L1-L11)

## Limits

| Limit | Value | Enforced by |
|---|---|---|
| name | 120 chars | zod + Rust `validate_for_save` |
| ssh target | 512 chars | zod + Rust `validate_for_save` |
| forwards | 16 | zod, core validation, Rust validation |
| jump hops | 8 | zod, core validation, Rust validation |
| extra options | 32 | zod, core validation, Rust validation |
| notes | 2000 chars | zod, core validation, Rust validation |
| profiles | 200 | manager (`maxProfiles`) |
| log lines | 2000 | HTTP clamp, Rust clamp in the Tauri command |
| log bytes | 128 KiB read window | both managers |

**Section sources**

- [packages/ssh-contract/src/tunnel.ts](file://packages/ssh-contract/src/tunnel.ts#L80-L115)
- [packages/ssh-core/src/validate.ts](file://packages/ssh-core/src/validate.ts#L20-L30)
- [crates/tunnel-core/src/validate.rs](file://crates/tunnel-core/src/validate.rs#L55-L60)
