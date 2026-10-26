/**
 * `tunneldeck-tui` — terminal front over the daemon API, authored in
 * Solid on the Uniview TUI engine. Pure JavaScript (no native binding), so the
 * same bundle runs on macOS, Linux and Windows. Mounts once; signal writes
 * drive every later frame.
 *
 *   tunneldeck-tui [--daemon=http://127.0.0.1:7432]
 */
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { createRoot, createSignal } from "solid-js";
import { AnsiCellSurface, StyleTable, TerminalDriver } from "@uniview/tui-core";
import { createTuiSolidRoot } from "@uniview/tui-solid";
import { createTunnelClient } from "@ssh-tunnel/client";
import { App } from "./app.js";
import { backendFromClient } from "./backend.js";
import { handleKey } from "./keys.js";
import { createTunnelModel } from "./model.js";

const REFRESH_MS = 2_500;

function daemonBase(): string {
  for (const arg of process.argv.slice(2)) {
    if (arg.startsWith("--daemon=")) return arg.slice("--daemon=".length).replace(/\/$/, "");
  }
  try {
    const raw = readFileSync(join(homedir(), ".ssh-tunnel-ui", "daemon.json"), "utf8");
    const parsed = JSON.parse(raw) as { host?: unknown; port?: unknown };
    if (typeof parsed.host === "string" && typeof parsed.port === "number") {
      return `http://${parsed.host}:${parsed.port}`;
    }
  } catch {
    // No discovery file: use the default below.
  }
  return "http://127.0.0.1:7432";
}

const rows = (): number => process.stdout.rows ?? 24;

const styles = new StyleTable();
const surface = new AnsiCellSurface({ write: (chunk) => process.stdout.write(chunk), styles });
const root = createTuiSolidRoot({
  surface,
  styles,
  size: { width: process.stdout.columns ?? 80, height: rows() },
});

const model = createRoot(() => createTunnelModel(backendFromClient(createTunnelClient({ baseUrl: daemonBase(), fetch }))));
const timer = setInterval(() => void model.refresh(), REFRESH_MS);

const [height, setHeight] = createSignal(rows());
const quit = (): void => {
  clearInterval(timer);
  root.destroy();
  driver.stop();
  process.exit(0);
};

const driver = new TerminalDriver({
  input: process.stdin,
  output: process.stdout,
  mouse: "off",
  onEvent: (event) => {
    if (event.type === "resize") {
      setHeight(event.height);
      root.host.renderer.resize({ width: event.width, height: event.height });
      return;
    }
    handleKey({ model, quit }, event);
  },
});

void model.refresh();
root.render(() => <App model={model} rows={height()} />);
driver.start();
process.stdin.on("end", quit);
