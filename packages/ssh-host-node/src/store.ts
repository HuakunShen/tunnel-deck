/**
 * Node `StorePort`: atomic JSON profile store. Writes go to a temp file plus
 * rename, so a crash never leaves half a store. Reads tolerate a missing file
 * (first run) but surface corrupt JSON as `StoreFailed` with the path.
 */
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { TunnelError } from "@ssh-tunnel/contract";
import { tunnelProfileSchema, type TunnelProfile } from "@ssh-tunnel/contract";
import { dataDir } from "./paths.js";
import type { StorePort } from "./ports.js";

function defaultStorePath(): string {
  return join(dataDir(), "profiles.json");
}

export function createFileStorePort(
  storePath = defaultStorePath(),
): StorePort & { readonly path: string } {
  return {
    path: storePath,

    async readProfiles(): Promise<readonly TunnelProfile[]> {
      let raw: string;
      try {
        raw = readFileSync(storePath, "utf8");
      } catch (error: unknown) {
        if ((error as { code?: unknown }).code === "ENOENT") return [];
        throw new TunnelError("StoreFailed", `cannot read store at ${storePath}`, {
          details: { path: storePath },
        });
      }
      let parsed: unknown;
      try {
        parsed = JSON.parse(raw) as unknown;
      } catch {
        throw new TunnelError("StoreFailed", `store JSON is corrupt at ${storePath}`, {
          details: { path: storePath },
        });
      }
      if (!Array.isArray(parsed)) {
        throw new TunnelError("StoreFailed", `store JSON must be an array at ${storePath}`, {
          details: { path: storePath },
        });
      }
      const profiles: TunnelProfile[] = [];
      for (const entry of parsed) {
        const checked = tunnelProfileSchema.safeParse(entry);
        if (!checked.success) continue;
        profiles.push(checked.data);
      }
      return profiles;
    },

    async writeProfiles(profiles: readonly TunnelProfile[]): Promise<void> {
      try {
        mkdirSync(dirname(storePath), { recursive: true });
        const tmp = `${storePath}.${process.pid}.tmp`;
        writeFileSync(tmp, `${JSON.stringify(profiles, null, 2)}\n`, "utf8");
        renameSync(tmp, storePath);
      } catch {
        throw new TunnelError("StoreFailed", `cannot write store at ${storePath}`, {
          details: { path: storePath },
        });
      }
    },
  };
}
