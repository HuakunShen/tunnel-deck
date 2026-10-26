/**
 * `@ssh-tunnel/desktop` — Tauri shell stub (thin front).
 *
 * Production shape: `tauri.conf.json` with `frontendDist: ../web/dist`; the
 * Rust side links `tunnel-core` in-process and exposes it through typed
 * `tunnel_*` commands. The Web UI reaches them through
 * `createInvokeTunnelClient`, so there is no sidecar, no loopback port, and
 * no JavaScript runtime to install. Until then, this module documents the
 * wiring and offers the discovery helper every shell reuses.
 */
import { createInvokeTunnelClient, type InvokeFn } from "@ssh-tunnel/client";

export interface DesktopShellOptions {
  readonly invoke: InvokeFn;
}

export function createDesktopClient(options: DesktopShellOptions): ReturnType<typeof createInvokeTunnelClient> {
  return createInvokeTunnelClient(options.invoke);
}

export const TAURI_WIRING_NOTES = [
  "tauri.conf.json: frontendDist ../web/dist, devUrl http://127.0.0.1:5173",
  "in-process: tunnel-core linked into the shell, tunnel_* Tauri commands",
  "no domain logic in the shell: all tunnel rules stay in tunnel-core",
] as const;
