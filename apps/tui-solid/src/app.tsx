/**
 * The tunnel console as Solid components: a list panel, a detail panel for the
 * selected tunnel, and a full-screen log view. Props are read inline, never
 * destructured, so signal reads stay reactive.
 */
import { For, Show, type JSX } from "solid-js";
import { Box, List, Panel, StatusBar, Text, listCounter } from "@uniview/tui-solid";
import type { TunnelProfile, TunnelState } from "@ssh-tunnel/contract";
import type { TunnelModel } from "./model.js";

const DOTS: Readonly<Record<string, string>> = {
  running: "●",
  degraded: "◐",
  failed: "✖",
  starting: "◌",
  stopping: "◌",
};

export function dotFor(status: string | undefined): string {
  return DOTS[status ?? "stopped"] ?? "○";
}

/** One list row: dot, name, mode and the first forward. */
export function rowLabel(profile: TunnelProfile, state: TunnelState | undefined): string {
  const first = profile.forwards[0];
  const forward = first === undefined ? "" : ` :${first.bindPort}`;
  return `${dotFor(state?.status)} ${profile.name} [${profile.mode}] ${profile.sshTarget}${forward}`;
}

function detailLines(profile: TunnelProfile | undefined, state: TunnelState | undefined): string[] {
  if (profile === undefined) return ["No tunnels yet. Add one with `ssh-tunnel add` or the web console."];
  const lines = [
    `${profile.name} → ${profile.sshTarget}`,
    `status: ${state?.status ?? "stopped"}${state?.origin === undefined ? "" : ` (${state.origin})`}`,
  ];
  if (state?.pid !== undefined) lines.push(`pid: ${state.pid}`);
  if (state?.startedAt !== undefined) lines.push(`since: ${state.startedAt}`);
  for (const forward of state?.forwards ?? []) {
    const latency = forward.latencyMs === undefined ? "" : ` ${forward.latencyMs}ms`;
    lines.push(`${forward.kind}:${forward.bindPort} ${forward.ok ? "ok" : "FAIL"}${latency}`);
  }
  if (state?.lastError !== undefined && state.lastError !== "") lines.push(`error: ${state.lastError}`);
  return lines;
}

function ListScreen(props: { model: TunnelModel }): JSX.Element {
  const model = props.model;
  const items = (): string[] =>
    model.profiles().map((profile) => rowLabel(profile, model.states().get(profile.id)));
  const lines = (): string[] => {
    const profile = model.current();
    return detailLines(profile, profile === undefined ? undefined : model.states().get(profile.id));
  };
  return (
    <Box flexDirection="column" width="100%" height="100%">
      <Panel
        title="Tunnels"
        focused
        flexGrow={1}
        footer={listCounter(model.selected(), model.profiles().length)}
        footerAlign="right"
      >
        <Show when={model.profiles().length > 0}>
          <List
            items={items()}
            selectedIndex={model.selected()}
            onSelect={model.select}
            width="100%"
          />
        </Show>
      </Panel>
      <Panel title="Detail" height={8}>
        <For each={lines()}>{(line) => <Text>{line}</Text>}</For>
      </Panel>
      <Show when={model.error() !== null}>
        <Text color="red">{model.error() ?? ""}</Text>
      </Show>
      <StatusBar items={[{ label: model.busy() ? "working…" : "TunnelDeck", keyHint: model.notice() }]} />
    </Box>
  );
}

function LogScreen(props: { model: TunnelModel; rows: number }): JSX.Element {
  const model = props.model;
  // Panel border and the status bar take four rows.
  const visible = (): readonly string[] => model.logLines().slice(-Math.max(1, props.rows - 4));
  return (
    <Box flexDirection="column" width="100%" height="100%">
      <Panel title={`Log — ${model.current()?.name ?? ""}`} focused flexGrow={1}>
        <For each={visible()}>{(line) => <Text>{line}</Text>}</For>
      </Panel>
      <StatusBar items={[{ label: "esc/q", keyHint: "back" }]} />
    </Box>
  );
}

export function App(props: { model: TunnelModel; rows: number }): JSX.Element {
  return (
    <Show when={props.model.screen() === "log"} fallback={<ListScreen model={props.model} />}>
      <LogScreen model={props.model} rows={props.rows} />
    </Show>
  );
}
