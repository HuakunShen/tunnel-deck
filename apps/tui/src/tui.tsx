/**
 * `ssh-tunnel-tui` — fullscreen terminal front over the same daemon API the
 * Web UI uses (or an in-process manager with `--direct`). Thin front: renders
 * states, sends start/stop/probe, never re-implements tunnel rules.
 */
import { createCliRenderer } from "@opentui/core";
import { createRoot, useKeyboard } from "@opentui/react";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { createTunnelClient, type TunnelClient } from "@ssh-tunnel/client";
import {
  TunnelError,
  type TunnelProfile,
  type TunnelState,
} from "@ssh-tunnel/contract";
import { TunnelListView } from "./view.js";
import { TunnelForm } from "./form.js";
import type { TunnelProfileInput } from "@ssh-tunnel/contract";
import { TunnelManager } from "@ssh-tunnel/service";
import {
  createFileStorePort,
  createNodeProbePort,
  createNodeProcessPort,
  systemClock,
  dataDir,
} from "@ssh-tunnel/host-node";
import { join } from "node:path";
import { readFileSync } from "node:fs";

function daemonBase(): string {
  for (const arg of process.argv.slice(2)) {
    if (arg.startsWith("--daemon=")) return arg.slice("--daemon=".length).replace(/\/$/, "");
  }
  try {
    const raw = readFileSync(join(dataDir(), "daemon.json"), "utf8");
    const parsed = JSON.parse(raw) as { host?: unknown; port?: unknown };
    if (typeof parsed.host === "string" && typeof parsed.port === "number") {
      return `http://${parsed.host}:${parsed.port}`;
    }
  } catch {
    // Default below.
  }
  return "http://127.0.0.1:7432";
}

interface Snapshot {
  readonly profiles: readonly TunnelProfile[];
  readonly states: readonly TunnelState[];
  readonly error: string | null;
}

function App(props: { client: TunnelClient | null; manager: TunnelManager | null }): ReactNode {
  const [snapshot, setSnapshot] = useState<Snapshot>({ profiles: [], states: [], error: null });
  const [selected, setSelected] = useState(0);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("q quit · ↑↓ select · enter start/stop · n new · d delete · p probe · s stop-all · r refresh");
  const [formOpen, setFormOpen] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const profiles =
        props.manager !== null ? await props.manager.listProfiles() : await props.client?.listProfiles() ?? [];
      const states =
        props.manager !== null ? await props.manager.listStates() : await props.client?.listStates() ?? [];
      setSnapshot({ profiles: [...profiles], states: [...states], error: null });
    } catch (error) {
      setSnapshot((prev) => ({
        ...prev,
        error: error instanceof TunnelError ? `${error.code}: ${error.message}` : "cannot reach backend",
      }));
    }
  }, [props.client, props.manager]);

  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 2500);
    return () => clearInterval(timer);
  }, [refresh]);

  const statesById = useMemo(
    () => new Map(snapshot.states.map((s) => [s.profileId, s])),
    [snapshot.states],
  );

  const act = useCallback(
    async (fn: () => Promise<unknown>, label: string) => {
      if (busy) return;
      setBusy(true);
      try {
        await fn();
        setNotice(label);
      } catch (error) {
        setNotice(error instanceof TunnelError ? `${error.code}: ${error.message}` : "action failed");
      } finally {
        setBusy(false);
        await refresh();
      }
    },
    [busy, refresh],
  );

  useKeyboard((key) => {
    if (key.name === "q" || (key.ctrl && key.name === "c")) {
      process.exit(0);
    }
    // While the form is open it owns ↑↓/esc/enter; only q quits globally.
    if (formOpen) return;
    if (snapshot.profiles.length === 0) {
      if (key.name === "r") void refresh();
      else if (key.name === "n") setFormOpen(true);
      return;
    }
    if (key.name === "up" || key.name === "k") {
      setSelected((s) => (s - 1 + snapshot.profiles.length) % snapshot.profiles.length);
    } else if (key.name === "down" || key.name === "j") {
      setSelected((s) => (s + 1) % snapshot.profiles.length);
    } else if (key.name === "return") {
      const profile = snapshot.profiles[selected];
      const state = profile !== undefined ? statesById.get(profile.id) : undefined;
      if (profile === undefined) return;
      if (state?.status === "running" || state?.status === "degraded") {
        void act(
          () => (props.manager !== null ? props.manager.stop(profile.id) : (props.client?.stop(profile.id) as Promise<unknown>)),
          `stopped ${profile.name}`,
        );
      } else {
        void act(
          () => (props.manager !== null ? props.manager.start(profile.id) : (props.client?.start(profile.id) as Promise<unknown>)),
          `started ${profile.name}`,
        );
      }
    } else if (key.name === "p") {
      const profile = snapshot.profiles[selected];
      if (profile === undefined) return;
      void act(
        () => (props.manager !== null ? props.manager.probe(profile.id) : (props.client?.probe(profile.id) as Promise<unknown>)),
        `probed ${profile.name}`,
      );
    } else if (key.name === "s") {
      void act(
        () => (props.manager !== null ? props.manager.stopAll() : (props.client?.stopAll() as Promise<unknown>)),
        "stopped all tunnels",
      );
    } else if (key.name === "n") {
      setFormOpen(true);
    } else if (key.name === "d") {
      const profile = snapshot.profiles[selected];
      if (profile === undefined) return;
      void act(
        () =>
          props.manager !== null
            ? props.manager.deleteProfile(profile.id)
            : (props.client?.deleteProfile(profile.id) as Promise<unknown>),
        `deleted ${profile.name}`,
      );
    } else if (key.name === "r") {
      void refresh();
    }
  });

  async function createTunnel(input: TunnelProfileInput): Promise<void> {
    setFormOpen(false);
    await act(
      () =>
        props.manager !== null
          ? props.manager.createProfile(input).then(() => undefined)
          : (props.client?.createProfile(input).then(() => undefined) as Promise<unknown>),
      `created ${input.name} — press enter to start`,
    );
  }
  if (formOpen) {
    return (
      <box style={{ flexDirection: "column", padding: 1, gap: 1 }}>
        <TunnelForm onSubmit={(input) => void createTunnel(input)} onCancel={() => setFormOpen(false)} />
      </box>
    );
  }

  return (
    <TunnelListView
      profiles={snapshot.profiles}
      statesById={statesById}
      selected={selected}
      busy={busy}
      error={snapshot.error}
      notice={notice}
    />
  );
}

async function main(): Promise<void> {
  const direct = process.argv.slice(2).includes("--direct");
  const base = dataDir();
  const manager = direct
    ? new TunnelManager({
        process: createNodeProcessPort(),
        probe: createNodeProbePort(),
        store: createFileStorePort(join(base, "profiles.json")),
        clock: systemClock,
        logDir: join(base, "logs"),
      })
    : null;
  const client = direct ? null : createTunnelClient({ baseUrl: daemonBase(), fetch });
  const renderer = await createCliRenderer();
  createRoot(renderer).render(<App client={client} manager={manager} />);
}

await main();
