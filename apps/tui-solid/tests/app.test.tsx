/**
 * Renders the app on a memory surface (no TTY, no native binding) against a
 * fake backend, and drives it through `handleKey` like the terminal would.
 */
import { afterEach, describe, expect, it } from "vitest";
import { createRoot } from "solid-js";
import { MemoryCellSurface, StyleTable, type TuiInputEvent } from "@uniview/tui-core";
import { createTuiSolidRoot } from "@uniview/tui-solid";
import type { TunnelProfile, TunnelState } from "@ssh-tunnel/contract";
import { App } from "../src/app";
import type { TunnelBackend } from "../src/backend";
import { handleKey } from "../src/keys";
import { createTunnelModel } from "../src/model";

const tick = async (): Promise<void> => {
  for (let i = 0; i < 25; i += 1) await new Promise<void>((resolve) => setTimeout(resolve, 0));
};

const profile = (id: string, name: string): TunnelProfile => ({
  id,
  name,
  mode: "local",
  sshTarget: "ufo",
  forwards: [{ kind: "local", bindPort: 18080, targetHost: "127.0.0.1", targetPort: 80 }],
  createdAt: "2026-01-01T00:00:00.000Z",
  updatedAt: "2026-01-01T00:00:00.000Z",
});

function fakeBackend() {
  const calls: string[] = [];
  const states = new Map<string, TunnelState>();
  const backend: TunnelBackend = {
    listProfiles: async () => [profile("a", "alpha"), profile("b", "beta")],
    listStates: async () => [...states.values()],
    start: async (id) => {
      calls.push(`start ${id}`);
      states.set(id, { profileId: id, status: "running", forwards: [] });
    },
    stop: async (id) => {
      calls.push(`stop ${id}`);
      states.delete(id);
    },
    probe: async (id) => void calls.push(`probe ${id}`),
    stopAll: async () => void calls.push("stopAll"),
    remove: async (id) => void calls.push(`remove ${id}`),
    log: async (id) => ({ profileId: id, path: "/x.log", lines: ["line one", "line two"], truncated: false, bytes: 1 }),
  };
  return { backend, calls };
}

// Only one TUI Solid root may be active at a time, so each test tears its own down.
const mounted: { destroy(): void }[] = [];
afterEach(() => {
  for (const root of mounted.splice(0)) root.destroy();
});

function mount() {
  const { backend, calls } = fakeBackend();
  const styles = new StyleTable();
  const surface = new MemoryCellSurface({ styles });
  const root = createTuiSolidRoot({ surface, styles, size: { width: 90, height: 24 } });
  mounted.push(root);
  const model = createRoot(() => createTunnelModel(backend));
  let quit = 0;
  root.render(() => <App model={model} rows={24} />);
  const press = async (input: TuiInputEvent): Promise<void> => {
    handleKey({ model, quit: () => void (quit += 1) }, input);
    await tick();
  };
  return { surface, model, calls, press, quits: () => quit };
}

const text = (t: string): TuiInputEvent => ({ type: "text", text: t });
const key = (k: string, ctrl = false): TuiInputEvent => ({
  type: "key",
  key: k,
  ctrl,
  alt: false,
  shift: false,
  meta: false,
});

describe("tunnel TUI", () => {
  it("lists tunnels and shows the selected one's detail", async () => {
    const { surface, model } = mount();
    await model.refresh();
    await tick();
    const out = surface.text({ trimRight: true });
    expect(out).toContain("alpha");
    expect(out).toContain("beta");
    expect(out).toContain("○ alpha");
  });

  it("moves the selection with arrows and vim keys, wrapping around", async () => {
    const { model, press } = mount();
    await model.refresh();
    await press(key("ArrowDown"));
    expect(model.selected()).toBe(1);
    await press(text("j"));
    expect(model.selected()).toBe(0);
    await press(text("k"));
    expect(model.selected()).toBe(1);
  });

  it("starts then stops the selected tunnel with enter", async () => {
    const { surface, model, calls, press } = mount();
    await model.refresh();
    await press(key("Enter"));
    expect(calls).toEqual(["start a"]);
    expect(surface.text({ trimRight: true })).toContain("● alpha");
    await press(key("Enter"));
    expect(calls).toEqual(["start a", "stop a"]);
  });

  it("needs d twice to delete, and any other key disarms it", async () => {
    const { model, calls, press } = mount();
    await model.refresh();
    await press(text("d"));
    expect(calls).toEqual([]);
    await press(text("j"));
    await press(text("d"));
    expect(calls).toEqual([]);
    await press(text("d"));
    expect(calls).toEqual(["remove b"]);
  });

  it("opens the log view and returns with escape", async () => {
    const { surface, model, press } = mount();
    await model.refresh();
    await press(text("l"));
    expect(surface.text({ trimRight: true })).toContain("line two");
    await press(key("Escape"));
    expect(surface.text({ trimRight: true })).toContain("Tunnels");
  });

  it("quits on q and Ctrl-C", async () => {
    const { press, quits } = mount();
    await press(text("q"));
    await press(key("c", true));
    expect(quits()).toBe(2);
  });
});
