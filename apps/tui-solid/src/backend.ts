/**
 * The slice of the daemon API the TUI uses. The model depends on this
 * interface rather than on `TunnelClient`, so tests drive it with a plain
 * object and no HTTP.
 */
import type { TunnelClient } from "@ssh-tunnel/client";
import type { LogTail, TunnelProfile, TunnelState } from "@ssh-tunnel/contract";

export interface TunnelBackend {
  listProfiles(): Promise<readonly TunnelProfile[]>;
  listStates(): Promise<readonly TunnelState[]>;
  start(id: string): Promise<unknown>;
  stop(id: string): Promise<unknown>;
  probe(id: string): Promise<unknown>;
  stopAll(): Promise<unknown>;
  remove(id: string): Promise<unknown>;
  log(id: string, tail: number): Promise<LogTail>;
}

export function backendFromClient(client: TunnelClient): TunnelBackend {
  return {
    listProfiles: () => client.listProfiles(),
    listStates: () => client.listStates(),
    start: (id) => client.start(id),
    stop: (id) => client.stop(id),
    probe: (id) => client.probe(id),
    stopAll: () => client.stopAll(),
    remove: (id) => client.deleteProfile(id),
    log: (id, tail) => client.getLog(id, tail),
  };
}
