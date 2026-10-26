# DSH Plugin

<cite>
**Referenced Files in This Document**
- [integrations/dsh/package.json](file://integrations/dsh/package.json)
- [integrations/dsh/src/host.ts](file://integrations/dsh/src/host.ts)
- [integrations/dsh/src/client.js](file://integrations/dsh/src/client.js)
- [integrations/dsh/scripts/build.ts](file://integrations/dsh/scripts/build.ts)
- [tests/contract/dsh.test.ts](file://tests/contract/dsh.test.ts)
- [packages/ssh-service/src/http.ts](file://packages/ssh-service/src/http.ts)
</cite>

## Table of Contents

1. [Shape](#shape)
2. [Host Half](#host-half)
3. [Routing and Guards](#routing-and-guards)
4. [Client Half](#client-half)
5. [Build](#build)
6. [Tests and Gaps](#tests-and-gaps)

## Shape

`integrations/dsh` is a DeepSeek Harness plugin with two halves:

- `src/host.ts` runs in the Harness **host** process, assembles a real
  `TunnelManager` + Hono app, mounts it at `/ssh-tunnel` on the Harness
  webServer, and serves the built Web console as the panel document;
- `src/client.js` runs in the Harness **Web UI**, registers a sidebar panel, and
  frames that document.

This follows the Refyard/Space Lens plugin shape: no second origin, no second
implementation of the tunnel rules. The package declares
`dsh.client.platform: "web"`, `immediately: true`, and a bundle patch
(`cordis.patch.yml`).

**Section sources**

- [integrations/dsh/package.json](file://integrations/dsh/package.json#L1-L55)
- [integrations/dsh/src/host.ts](file://integrations/dsh/src/host.ts#L1-L40)

## Host Half

`inject = ["webServer"]`, so the plugin applies once the server service exists.
`apply(ctx)`:

1. warns and returns if there is no `webServer`;
2. resolves the data directory from `SSH_TUNNEL_HOME` or
   `~/.ssh-tunnel-ui`;
3. builds a `TunnelManager` with process, probe, file store, clock and logDir;
4. builds `createServiceApp({ manager, serviceInstanceId: "dsh-plugin" })`;
5. serves documents from `../web` relative to the module (staged at build time);
6. registers a prefix route on the Harness server.

The manager here has **no** terminal, control, runtime or open-url port, so the
panel has the daemon's core CRUD/start/stop/probe/log/preview surface but not
terminal launch, browser opening or restart adoption.

**Section sources**

- [integrations/dsh/src/host.ts](file://integrations/dsh/src/host.ts#L55-L120)

## Routing and Guards

- `GET /ssh-tunnel/dsh/context` → `{ panelUrl: "/ssh-tunnel/" }`.
- `GET /ssh-tunnel/*` → a file under `web/`, with an SPA fallback to
  `index.html`; `..` is rejected.
- `/ssh-tunnel/api/*` → forwarded to the in-process Hono app by constructing a
  synthetic `Request` with the prefix stripped, then streaming the response body
  back.

Every request is checked first:

1. the Harness webServer must have a positive port, else `503`;
2. `Host` must equal `http://127.0.0.1:<port>` or `http://localhost:<port>`,
   else `403`;
3. `Origin: null` is refused;
4. a non-empty Origin must be in the loopback set, else `403`;
5. an origin-less request with `Sec-Fetch-Site: cross-site` is refused.

Only string headers are copied into the synthetic `Request`; arrays are joined,
other values dropped, because Node's raw header table is wider than the Fetch
`HeadersInit` type.

**Section sources**

- [integrations/dsh/src/host.ts](file://integrations/dsh/src/host.ts#L120-L235)

## Client Half

`client.js` loads through `window.__ModuleLoader__.load`, requires React from
the host, and:

- registers a panel whose id/kind is `ssh-tunnel`;
- fetches `/ssh-tunnel/dsh/context` to resolve the frame URL;
- renders an `<iframe>` titled "SSH tunnel manager";
- has English and Chinese dictionaries (`panel`, `loading`, `failed`, `retry`,
  `frameTitle`), defaulting to English when no translator is bound;
- draws an inline SVG tunnel icon.

Because the frame is same-origin with the Harness page, it inherits the Harness
session; there is no token bridge.

**Section sources**

- [integrations/dsh/src/client.js](file://integrations/dsh/src/client.js#L1-L60)

## Build

`bun ./scripts/build.ts` copies `apps/web/dist` to `integrations/dsh/web/` (the
directory is gitignored). Without it, `/api/*` works but the panel document
404s. Turbo orders this build after `@ssh-tunnel/web#build`, and CI runs
`pnpm build` before `pnpm test:ci` because a contract test serves the staged
document.

**Section sources**

- [integrations/dsh/scripts/build.ts](file://integrations/dsh/scripts/build.ts#L1-L25)
- [turbo.json](file://turbo.json#L20-L30)
- [.github/workflows/ci.yml](file://.github/workflows/ci.yml#L36-L45)

## Tests and Gaps

`tests/contract/dsh.test.ts` drives `apply` against a fake webServer and fake
Node request/response streams. It asserts the mount, the context route, CRUD
proxy behaviour, and `403` for a non-loopback Host/Origin.

Gaps:

1. **Fewer ports than the daemon** (see above), so the panel UI may show
   terminal/browser affordances that fail with `UnsupportedOperation`.
2. **No auth check of its own**; it relies on being mounted on the Harness
   loopback server and on the header guards.
3. **Staged document is gitignored**, so a fresh checkout must build before the
   panel works.
4. **Client half is plain JS**, not TypeScript, and is not type-checked by the
   package's `tsc --noEmit`.
5. **No test for the client half**; only the host half is covered.

**Section sources**

- [tests/contract/dsh.test.ts](file://tests/contract/dsh.test.ts#L1-L60)
- [integrations/dsh/src/client.js](file://integrations/dsh/src/client.js#L1-L20)
