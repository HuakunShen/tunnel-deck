/**
 * Read-only SSH config inventory: parses the user's own `~/.ssh/config`
 * into alias → {HostName, User, Port}. Carries no connection material — the
 * manager only offers aliases for completion, and OpenSSH still decides the
 * rest at spawn time. Never throws: any I/O problem yields `[]`.
 */
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { basename, dirname, isAbsolute, join } from "node:path";
import type { SshHostInfo } from "@ssh-tunnel/contract";
import { userHome } from "./paths.js";

interface ConfigBlock {
  readonly patterns: readonly string[];
  readonly hostName?: string;
  readonly user?: string;
  readonly port?: number;
}

function parseConfigText(text: string): ConfigBlock[] {
  const blocks: ConfigBlock[] = [];
  let patterns: string[] | null = null;
  let hostName: string | undefined;
  let user: string | undefined;
  let port: number | undefined;
  const flush = (): void => {
    if (patterns !== null) {
      blocks.push({
        patterns,
        ...(hostName === undefined ? {} : { hostName }),
        ...(user === undefined ? {} : { user }),
        ...(port === undefined ? {} : { port }),
      });
    }
  };
  for (const rawLine of text.split("\n")) {
    const line = rawLine.trim();
    if (line === "" || line.startsWith("#")) continue;
    const match =
      /^([A-Za-z]+)\s*=\s*(.*?)\s*$/.exec(line) ??
      /^([A-Za-z]+)\s+(.*?)\s*$/.exec(line);
    if (match === null) continue;
    const key = (match[1] ?? "").toLowerCase();
    const value = match[2] ?? "";
    if (key === "host") {
      flush();
      patterns = value.split(/\s+/).filter((p) => p !== "");
      hostName = undefined;
      user = undefined;
      port = undefined;
    } else if (patterns !== null) {
      if (key === "hostname" && hostName === undefined) hostName = value;
      else if (key === "user" && user === undefined) user = value;
      else if (key === "port" && port === undefined) {
        const parsed = Number.parseInt(value, 10);
        if (Number.isInteger(parsed) && parsed >= 1 && parsed <= 65535) {
          port = parsed;
        }
      }
    }
  }
  flush();
  return blocks;
}

function patternMatches(pattern: string, alias: string): boolean {
  if (pattern === "*") return true;
  if (!pattern.includes("*") && !pattern.includes("?")) return pattern === alias;
  const regex = new RegExp(
    `^${pattern
      .replace(/[.+^${}()|[\]\\]/g, "\\$&")
      .replace(/\*/g, ".*")
      .replace(/\?/g, ".")}$`,
  );
  return regex.test(alias);
}

/** First-obtained-value wins, mirroring OpenSSH parameter resolution. */
function resolveAlias(
  blocks: readonly ConfigBlock[],
  alias: string,
): { hostName?: string; user?: string; port?: number } {
  let hostName: string | undefined;
  let user: string | undefined;
  let port: number | undefined;
  for (const block of blocks) {
    if (!block.patterns.some((pattern) => patternMatches(pattern, alias))) {
      continue;
    }
    if (hostName === undefined) hostName = block.hostName;
    if (user === undefined) user = block.user;
    if (port === undefined) port = block.port;
  }
  return {
    ...(hostName === undefined ? {} : { hostName }),
    ...(user === undefined ? {} : { user }),
    ...(port === undefined ? {} : { port }),
  };
}

function simpleGlob(pattern: string): string[] {
  // Supports one `*` in the basename only — enough for `config.d/*`.
  if (!pattern.includes("*")) return existsSync(pattern) ? [pattern] : [];
  const dir = dirname(pattern);
  const base = basename(pattern);
  const regex = new RegExp(
    `^${base.replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*")}$`,
  );
  let entries: string[];
  try {
    entries = readdirSync(dir);
  } catch {
    return [];
  }
  return entries
    .filter((entry) => regex.test(entry))
    .map((entry) => join(dir, entry))
    .sort();
}

/** Read aliases from `~/.ssh/config` (plus one Include level, no recursion). */
export function readSshConfigHosts(
  home: string = userHome(),
): SshHostInfo[] {
  try {
    const main = join(home, ".ssh", "config");
    if (!existsSync(main)) return [];
    const mainText = readFileSync(main, "utf8");
    const texts: string[] = [mainText];
    const includeRegex = /^\s*Include\s+(.+?)\s*$/gim;
    let includeMatch: RegExpExecArray | null;
    while ((includeMatch = includeRegex.exec(mainText)) !== null) {
      for (const piece of (includeMatch[1] ?? "").trim().split(/\s+/)) {
        if (piece === "") continue;
        const expanded = isAbsolute(piece)
          ? piece
          : piece.startsWith("~/")
            ? join(home, piece.slice(2))
            : join(home, ".ssh", piece);
        for (const file of simpleGlob(expanded)) {
          try {
            texts.push(readFileSync(file, "utf8"));
          } catch {
            // Skip unreadable includes.
          }
        }
      }
    }
    const blocks = texts.flatMap((text) => parseConfigText(text));
    const aliases = new Set<string>();
    for (const block of blocks) {
      for (const pattern of block.patterns) {
        if (pattern === "*" || pattern.includes("*") || pattern.includes("?")) {
          continue;
        }
        aliases.add(pattern);
      }
    }
    return [...aliases]
      .sort((a, b) => a.localeCompare(b))
      .map((alias) => ({ alias, ...resolveAlias(blocks, alias) }));
  } catch {
    return [];
  }
}
