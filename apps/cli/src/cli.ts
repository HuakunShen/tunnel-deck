/**
 * `ssh-tunnel` CLI — thin front over the daemon API, with a `--direct` mode
 * that embeds the manager in-process (standalone, no daemon needed).
 * Composition + argv parsing only; all rules live in core/service.
 */
import {
  createNodeProbePort,
  createNodeProcessPort,
  createFileStorePort,
  dataDir,
  systemClock,
} from "@ssh-tunnel/host-node";
import { createTunnelClient } from "@ssh-tunnel/client";
import { TunnelManager } from "@ssh-tunnel/service";
import type { TunnelProfileInput } from "@ssh-tunnel/contract";
import { profileInputSchema } from "@ssh-tunnel/contract";
import { readFileSync } from "node:fs";
import { join } from "node:path";

function daemonBase(): string {
  // `ssh-tunnel --daemon=http://x` overrides; else the discovery file; else default.
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
    // Fall through to default.
  }
  return "http://127.0.0.1:7432";
}

async function directManager(): Promise<TunnelManager> {
  const base = dataDir();
  return new TunnelManager({
    process: createNodeProcessPort(),
    probe: createNodeProbePort(),
    store: createFileStorePort(join(base, "profiles.json")),
    clock: systemClock,
    logDir: join(base, "logs"),
  });
}

function isDirect(): boolean {
  return process.argv.slice(2).includes("--direct");
}

function positionalArgs(): string[] {
  return process.argv.slice(2).filter((arg) => !arg.startsWith("--"));
}

function flagValue(name: string): string | undefined {
  const prefix = `--${name}=`;
  for (const arg of process.argv.slice(2)) {
    if (arg.startsWith(prefix)) return arg.slice(prefix.length);
  }
  return undefined;
}

function printStatus(status: string): string {
  const dot =
    status === "running" ? "●" : status === "degraded" ? "◐" : status === "failed" ? "✖" : "○";
  return `${dot} ${status}`;
}

async function main(): Promise<void> {
  const [command, target, extra] = positionalArgs();
  if (command === undefined || command === "help" || command === "--help") {
    console.log(`ssh-tunnel — one-click SSH tunnel manager
usage:
  ssh-tunnel list [--direct|--daemon=URL]
  ssh-tunnel add --file=profile.json [--direct|--daemon=URL]
  ssh-tunnel show <id> [--direct|--daemon=URL]
  ssh-tunnel preview <id> [--direct|--daemon=URL]
  ssh-tunnel start <id> [--direct|--daemon=URL]
  ssh-tunnel stop <id> [--direct|--daemon=URL]
  ssh-tunnel stop-all [--direct|--daemon=URL]
  ssh-tunnel probe <id> [--direct|--daemon=URL]
  ssh-tunnel delete <id> [--direct|--daemon=URL]
  ssh-tunnel hosts [--direct|--daemon=URL]`);
    return;
  }

  const direct = isDirect();
  const client = direct ? null : createTunnelClient({ baseUrl: daemonBase(), fetch });
  const manager = direct ? await directManager() : null;

  const needId = ["show", "preview", "logs", "start", "stop", "probe", "delete"].includes(command);
  if (needId && target === undefined) {
    console.error(`usage: ssh-tunnel ${command} <id>`);
    process.exitCode = 1;
    return;
  }

  try {
    switch (command) {
      case "list": {
        const profiles = manager !== null ? await manager.listProfiles() : await client?.listProfiles();
        const states = manager !== null ? await manager.listStates() : await client?.listStates();
        const byId = new Map((states ?? []).map((s) => [s.profileId, s.status]));
        for (const profile of profiles ?? []) {
          console.log(
            `${profile.id}\t${printStatus(byId.get(profile.id) ?? "stopped")}\t${profile.name}\t${profile.sshTarget}`,
          );
        }
        break;
      }
      case "add": {
        const file = flagValue("file") ?? extra;
        if (file === undefined) {
          console.error("usage: ssh-tunnel add --file=profile.json");
          process.exitCode = 1;
          return;
        }
        const raw = JSON.parse(readFileSync(file, "utf8") as string) as unknown;
        const parsed = profileInputSchema.safeParse(raw);
        if (!parsed.success) {
          console.error("profile JSON is malformed");
          process.exitCode = 1;
          return;
        }
        const input: TunnelProfileInput = parsed.data;
        const created =
          manager !== null ? await manager.createProfile(input) : await client?.createProfile(input);
        console.log(`created ${created?.id ?? ""} (${created?.name ?? ""})`);
        break;
      }
      case "show": {
        const profile =
          manager !== null
            ? await manager.getProfile(target as string)
            : await client?.getProfile(target as string);
        console.log(JSON.stringify(profile, null, 2));
        break;
      }
      case "preview": {
        if (manager !== null) {
          const { command: cmd } = manager.preview(target as string);
          console.log(cmd);
        } else {
          const result = await client?.preview(target as string);
          console.log(result?.command ?? "");
        }
        break;
      }
      case "logs": {
        const tail = Number(flagValue("tail") ?? "200");
        const count = Number.isFinite(tail) ? Math.min(2000, Math.max(1, Math.trunc(tail))) : 200;
        const log =
          manager !== null
            ? await manager.logTail(target as string, count)
            : await client?.getLog(target as string, count);
        if (log === undefined) {
          console.error("no daemon answered");
          process.exitCode = 1;
          return;
        }
        console.error(`${log.path} (${log.bytes} bytes${log.truncated ? ", tail" : ""})`);
        for (const line of log.lines) console.log(line);
        break;
      }
      case "start": {
        const state =
          manager !== null
            ? await manager.start(target as string)
            : await client?.start(target as string);
        console.log(`${target}: ${state?.status ?? "unknown"}`);
        if ((state?.lastError ?? "") !== "") console.log(`note: ${state?.lastError ?? ""}`);
        break;
      }
      case "stop": {
        const state =
          manager !== null
            ? await manager.stop(target as string)
            : await client?.stop(target as string);
        console.log(`${target}: ${state?.status ?? "unknown"}`);
        break;
      }
      case "stop-all": {
        const states = manager !== null ? await manager.stopAll() : await client?.stopAll();
        console.log(`stopped ${states?.length ?? 0} tunnel(s)`);
        break;
      }
      case "probe": {
        const state =
          manager !== null
            ? await manager.probe(target as string)
            : await client?.probe(target as string);
        for (const forward of state?.forwards ?? []) {
          console.log(`${forward.kind}:${forward.bindPort} ${forward.ok ? "ok" : "FAIL"}${forward.latencyMs !== undefined ? ` ${forward.latencyMs}ms` : ""}${forward.detail ? ` — ${forward.detail}` : ""}`);
        }
        break;
      }
      case "delete": {
        if (manager !== null) await manager.deleteProfile(target as string);
        else await client?.deleteProfile(target as string);
        console.log(`deleted ${target ?? ""}`);
        break;
      }
      default: {
        console.error(`unknown command ${command ?? ""}; see ssh-tunnel help`);
        process.exitCode = 1;
      }
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : "failed";
    console.error(`error: ${message}`);
    process.exitCode = 1;
  }
}

await main();
