/**
 * Runtime records on disk: what was up when the daemon last wrote the file.
 *
 * This is deliberately *not* the profile store. Profiles are the user's
 * configuration and a broken file must be reported; this is a disposable cache
 * of running tunnels, so every read failure degrades to "nothing was running"
 * and every write failure is swallowed. Losing it costs one honest inference
 * from a listening port, never a failed start.
 *
 * Writes go through a temp file plus rename like the profile store, and the
 * file is created 0600: it names hosts and pids, nothing secret, but there is
 * no reason for it to be world-readable either.
 */
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

import { dataDir } from "./paths.js";
import type { RuntimeRecord, RuntimeStorePort } from "./ports.js";

const ORIGINS = new Set(["managed", "terminal"]);
const MODES = new Set(["tunnel", "shell"]);

function defaultRuntimePath(): string {
  return join(dataDir(), "runtime.json");
}

/** Keep only entries that still look like a record; a drifted file is dropped. */
function parseRecords(raw: string): RuntimeRecord[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw) as unknown;
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const records: RuntimeRecord[] = [];
  for (const entry of parsed) {
    if (entry === null || typeof entry !== "object") continue;
    const value = entry as Record<string, unknown>;
    const profileId = value["profileId"];
    const origin = value["origin"];
    const startedAt = value["startedAt"];
    if (typeof profileId !== "string" || profileId === "") continue;
    if (typeof origin !== "string" || !ORIGINS.has(origin)) continue;
    if (typeof startedAt !== "string" || startedAt === "") continue;
    const pid = value["pid"];
    const mode = value["mode"];
    const terminalId = value["terminalId"];
    records.push({
      profileId,
      origin: origin as RuntimeRecord["origin"],
      startedAt,
      ...(typeof pid === "number" && Number.isInteger(pid) && pid > 0 ? { pid } : {}),
      ...(typeof mode === "string" && MODES.has(mode)
        ? { mode: mode as NonNullable<RuntimeRecord["mode"]> }
        : {}),
      ...(typeof terminalId === "string" && terminalId !== "" ? { terminalId } : {}),
    });
  }
  return records;
}

export function createFileRuntimeStore(
  runtimePath = defaultRuntimePath(),
): RuntimeStorePort & { readonly path: string } {
  return {
    path: runtimePath,

    async read(): Promise<readonly RuntimeRecord[]> {
      try {
        return parseRecords(readFileSync(runtimePath, "utf8"));
      } catch {
        // Missing, unreadable or corrupt: treat it as "nothing was running".
        return [];
      }
    },

    async write(records: readonly RuntimeRecord[]): Promise<void> {
      try {
        mkdirSync(dirname(runtimePath), { recursive: true });
        const tmp = `${runtimePath}.${process.pid}.tmp`;
        writeFileSync(tmp, `${JSON.stringify(records, null, 2)}\n`, {
          encoding: "utf8",
          mode: 0o600,
        });
        renameSync(tmp, runtimePath);
      } catch {
        // The record is a convenience; a read-only home must not stop a tunnel.
      }
    },
  };
}
