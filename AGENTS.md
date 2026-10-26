# TunnelDeck agent handbook

## Directory responsibilities

| Path | May import | Must never |
|---|---|---|
| `packages/ssh-contract` | `zod` only | `node:*`, DOM, other workspace packages |
| `packages/ssh-core` | `ssh-contract`, `zod` | `node:*`, DOM, host packages |
| `packages/ssh-client` | `ssh-contract` | `node:*` (caller injects `fetch`) |
| `packages/ssh-host-node` | `ssh-contract`, `ssh-core`, node builtins | DOM, apps |
| `packages/ssh-service` | all packages, `hono` | DOM |
| `apps/*` | packages only (composition) | each other; no domain logic |
| `integrations/*` | `ssh-contract`, `ssh-client` | `node:*` in shipped client code |

Boundaries are enforced by `scripts/check-boundaries.ts`
(`pnpm check:boundaries`), which reports `file:line:column`.

## Conventions

- TS `strict` + `noUncheckedIndexedAccess` + `verbatimModuleSyntax` +
  `noUnusedLocals/Parameters`, `noEmit: true` (check) / esbuild (bundle).
- Literals `as const` once, types via `typeof`. No `enum`.
- Polymorphic data = discriminated union on `kind`/`type`/`status`.
- HTTP boundary: requests `strictObject`, responses `.loose()` for compat.
- Errors: closed `TUNNEL_PROBLEM_CODES` + `TunnelError(code)`; clients branch
  on `code`, never on `message`. One code = one HTTP status (exhaustive record).
- Every source file opens with a module header comment (English).
- Zero `any` / `as unknown as` / `@ts-ignore` in handwritten code.
- Commits: `type(scope): lowercase complete english sentence`.
