/**
 * TUI state as Solid signals plus the actions that change it. Everything the
 * view shows lives here and every backend call goes through `TunnelBackend`;
 * the view only reads accessors and the key handler only calls actions. No
 * tunnel rules are re-implemented: the daemon decides, this reports.
 */
import { createSignal, type Accessor } from "solid-js";
import { TunnelError, type TunnelProfile, type TunnelState } from "@ssh-tunnel/contract";
import type { TunnelBackend } from "./backend.js";

export const LOG_TAIL_LINES = 200;

const IDLE_NOTICE = "enter start/stop · p probe · l logs · d delete · s stop all · r refresh · q quit";

export type Screen = "list" | "log";

export interface TunnelModel {
  readonly profiles: Accessor<readonly TunnelProfile[]>;
  readonly states: Accessor<ReadonlyMap<string, TunnelState>>;
  readonly selected: Accessor<number>;
  readonly screen: Accessor<Screen>;
  readonly logLines: Accessor<readonly string[]>;
  readonly notice: Accessor<string>;
  readonly error: Accessor<string | null>;
  readonly busy: Accessor<boolean>;
  /** The profile id armed for deletion (a second `d` confirms). */
  readonly pendingDelete: Accessor<string | null>;
  current(): TunnelProfile | undefined;
  refresh(): Promise<void>;
  move(delta: number): void;
  select(index: number): void;
  toggle(): Promise<void>;
  probe(): Promise<void>;
  stopAll(): Promise<void>;
  requestDelete(): Promise<void>;
  cancelDelete(): void;
  openLog(): Promise<void>;
  closeLog(): void;
}

function describe(error: unknown): string {
  // Clients branch on the closed `code`, never on the message text.
  if (error instanceof TunnelError) return `${error.code}: ${error.message}`;
  return error instanceof Error ? error.message : "action failed";
}

export function createTunnelModel(backend: TunnelBackend): TunnelModel {
  const [profiles, setProfiles] = createSignal<readonly TunnelProfile[]>([]);
  const [states, setStates] = createSignal<ReadonlyMap<string, TunnelState>>(new Map());
  const [selected, setSelected] = createSignal(0);
  const [screen, setScreen] = createSignal<Screen>("list");
  const [logLines, setLogLines] = createSignal<readonly string[]>([]);
  const [notice, setNotice] = createSignal(IDLE_NOTICE);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [pendingDelete, setPendingDelete] = createSignal<string | null>(null);

  const current = (): TunnelProfile | undefined => profiles()[selected()];

  async function refresh(): Promise<void> {
    try {
      const [nextProfiles, nextStates] = await Promise.all([
        backend.listProfiles(),
        backend.listStates(),
      ]);
      setProfiles(nextProfiles);
      setStates(new Map(nextStates.map((state) => [state.profileId, state])));
      setSelected((index) => Math.min(index, Math.max(0, nextProfiles.length - 1)));
      setError(null);
    } catch (cause) {
      setError(cause instanceof TunnelError ? describe(cause) : "cannot reach the daemon");
    }
  }

  /** Run one backend action, report its outcome, then re-read the daemon. */
  async function act(label: string, run: () => Promise<unknown>): Promise<void> {
    if (busy()) return;
    setBusy(true);
    try {
      await run();
      setNotice(label);
    } catch (cause) {
      setNotice(describe(cause));
    } finally {
      setBusy(false);
      await refresh();
    }
  }

  return {
    profiles,
    states,
    selected,
    screen,
    logLines,
    notice,
    error,
    busy,
    pendingDelete,
    current,
    refresh,
    move(delta) {
      const count = profiles().length;
      if (count === 0) return;
      setSelected((index) => (index + delta + count) % count);
    },
    select(index) {
      setSelected(Math.max(0, Math.min(index, profiles().length - 1)));
    },
    async toggle() {
      const profile = current();
      if (profile === undefined) return;
      const status = states().get(profile.id)?.status;
      if (status === "running" || status === "degraded") {
        await act(`stopped ${profile.name}`, () => backend.stop(profile.id));
      } else {
        await act(`started ${profile.name}`, () => backend.start(profile.id));
      }
    },
    async probe() {
      const profile = current();
      if (profile === undefined) return;
      await act(`probed ${profile.name}`, () => backend.probe(profile.id));
    },
    async stopAll() {
      await act("stopped all tunnels", () => backend.stopAll());
    },
    async requestDelete() {
      const profile = current();
      if (profile === undefined) return;
      if (pendingDelete() !== profile.id) {
        setPendingDelete(profile.id);
        setNotice(`press d again to delete ${profile.name} (any other key cancels)`);
        return;
      }
      setPendingDelete(null);
      await act(`deleted ${profile.name}`, () => backend.remove(profile.id));
    },
    cancelDelete() {
      if (pendingDelete() === null) return;
      setPendingDelete(null);
      setNotice(IDLE_NOTICE);
    },
    async openLog() {
      const profile = current();
      if (profile === undefined) return;
      try {
        const tail = await backend.log(profile.id, LOG_TAIL_LINES);
        setLogLines(tail.lines);
        setScreen("log");
      } catch (cause) {
        setNotice(describe(cause));
      }
    },
    closeLog() {
      setScreen("list");
    },
  };
}
