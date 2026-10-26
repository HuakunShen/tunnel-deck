/**
 * VS Code extension entry (thin front): activates the tunnel sidebar, wires
 * start/stop/probe commands to the shared typed client, and opens the
 * manager Web UI in a webview panel. The daemon is attached when healthy,
 * spawned when not — see `supervisor.ts`. No tunnel rules live here.
 */
import * as vscode from "vscode";
import { createTunnelClient, type TunnelClient } from "@ssh-tunnel/client";
import { ensureDaemon, type RunningDaemon } from "./supervisor.js";
import { TunnelTreeProvider, type VscodeApi } from "./tree.js";
import type { TunnelRow } from "./tree.js";

function daemonUrl(): string {
  const configured = vscode.workspace
    .getConfiguration("sshTunnel")
    .get<string>("daemonUrl", "");
  if (configured.trim() !== "") return configured.trim().replace(/\/$/, "");
  return "http://127.0.0.1:7432";
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const output = vscode.window.createOutputChannel("TunnelDeck");
  context.subscriptions.push(output);

  let running: RunningDaemon | null = null;
  try {
    running = await ensureDaemon({
      daemonUrl: daemonUrl(),
      serverDir: __dirname,
    });
    output.appendLine(`daemon: ${running.readiness.url} (${running.readiness.serviceInstanceId})`);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    void vscode.window.showErrorMessage(`TunnelDeck: ${message}`);
    output.appendLine(`daemon failed: ${message}`);
  }
  if (running?.owned === true) {
    const owned: RunningDaemon = running;
    context.subscriptions.push({ dispose(): void { owned.stop(); } });
  }
  const url = running?.readiness.url ?? daemonUrl();
  const client = createTunnelClient({ baseUrl: url, fetch });

  // Structural cast: the provider only needs the TreeItem/ThemeIcon/Event
  // surface, declared in tree.ts as VscodeApi so tests stay vscode-free.
  const provider = new TunnelTreeProvider(vscode as unknown as VscodeApi, url, fetch);
  context.subscriptions.push(
    vscode.window.registerTreeDataProvider("sshTunnel.tunnels", provider as unknown as vscode.TreeDataProvider<TunnelRow>),
  );
  const poll = setInterval(() => void provider.refresh(), 3000);
  context.subscriptions.push({ dispose(): void { clearInterval(poll); } });
  void provider.refresh();

  async function withClient<T>(fn: (client: TunnelClient) => Promise<T>, done: string): Promise<void> {
    try {
      await fn(client);
      output.appendLine(done);
      await provider.refresh();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      void vscode.window.showErrorMessage(`TunnelDeck: ${message}`);
    }
  }

  context.subscriptions.push(
    vscode.commands.registerCommand("ssh-tunnel.start", (row?: TunnelRow) =>
      pickRow(row, provider).then((picked) => {
        if (picked !== undefined) void withClient((c) => c.start(picked.profile.id), `started ${picked.profile.name}`);
      }),
    ),
    vscode.commands.registerCommand("ssh-tunnel.stop", (row?: TunnelRow) =>
      pickRow(row, provider).then((picked) => {
        if (picked !== undefined) void withClient((c) => c.stop(picked.profile.id), `stopped ${picked.profile.name}`);
      }),
    ),
    vscode.commands.registerCommand("ssh-tunnel.probe", (row?: TunnelRow) =>
      pickRow(row, provider).then((picked) => {
        if (picked !== undefined) void withClient((c) => c.probe(picked.profile.id), `probed ${picked.profile.name}`);
      }),
    ),
    vscode.commands.registerCommand("ssh-tunnel.stopAll", () =>
      withClient((c) => c.stopAll().then(() => undefined), "stopped all tunnels"),
    ),
    vscode.commands.registerCommand("ssh-tunnel.refresh", () => provider.refresh()),
    vscode.commands.registerCommand("ssh-tunnel.openWorkbench", () => openWorkbench(context, url)),
  );
}

async function pickRow(
  row: TunnelRow | undefined,
  provider: TunnelTreeProvider,
): Promise<TunnelRow | undefined> {
  if (row?.profile !== undefined) return row;
  const rows = provider.getChildren();
  if (rows.length === 0) {
    void vscode.window.showInformationMessage("TunnelDeck: no tunnels yet — open the workbench to add one.");
    return undefined;
  }
  const picked = await vscode.window.showQuickPick(
    rows.map((candidate) => ({
      label: `${candidate.profile.name} [${candidate.status}]`,
      description: candidate.detail,
      row: candidate,
    })),
    { placeHolder: "Pick a tunnel" },
  );
  return picked?.row;
}

function openWorkbench(context: vscode.ExtensionContext, url: string): void {
  const panel = vscode.window.createWebviewPanel(
    "sshTunnelWorkbench",
    "TunnelDeck",
    vscode.ViewColumn.One,
    { enableScripts: true, retainContextWhenHidden: true },
  );
  panel.webview.html = `<!doctype html><html><body style="margin:0;height:100vh">
<iframe src="${url}/" style="border:none;width:100%;height:100vh" title="SSH tunnel manager"></iframe>
</body></html>`;
  context.subscriptions.push(panel);
}

export function deactivate(): void {
  // Disposables above stop the owned daemon and the poll timer.
}
