#!/usr/bin/env node
/**
 * Write the Tauri updater feed (`latest.json`) for one release.
 *
 * The updater client polls a single static file; each platform entry names
 * the signed archive for that platform plus the minisign signature over its
 * bytes. File names are the friendly ones `collect-bundles.mjs` produced —
 * the signature covers content, never the name, so renaming is safe as long
 * as the URLs here match the uploaded asset names exactly.
 *
 * Usage: `node scripts/render-updater-json.mjs --tag v0.2.0 --dir dist [--out dist/latest.json]`
 */
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function arg(name, fallback) {
  const index = process.argv.indexOf(`--${name}`);
  if (index === -1 || process.argv[index + 1] === undefined) return fallback;
  return process.argv[index + 1];
}

/**
 * Release label → the platform key the updater client sends, plus the asset
 * tail. Only macOS wraps its bundle (`.app.tar.gz`); the v2 updater installs
 * AppImage and the nsis `.exe` directly, so those feed entries ARE the
 * friendly downloads.
 */
const PLATFORM = {
  "macos-arm64": ["darwin-aarch64", ".app.tar.gz"],
  "macos-x64": ["darwin-x86_64", ".app.tar.gz"],
  "linux-x64": ["linux-x86_64", ".AppImage"],
  "windows-x64": ["windows-x86_64", "-setup.exe"],
};

const tag = arg("tag");
if (tag === undefined || !tag.startsWith("v")) {
  console.error("usage: render-updater-json.mjs --tag <vSemver> [--dir <dir>] [--out <file>]");
  process.exit(2);
}
const version = tag.replace(/^v/, "");
const dir = resolve(root, arg("dir", "dist"));
const out = resolve(root, arg("out", join(dir, "latest.json")));

const files = new Set(readdirSync(dir));
const platforms = {};
for (const [label, [platform, suffix]] of Object.entries(PLATFORM)) {
  const archive = `TunnelDeck-${version}-${label}${suffix}`;
  const sigFile = `${archive}.sig`;
  if (!files.has(archive) || !files.has(sigFile)) {
    console.error(`[updater] missing ${archive} or its .sig — skipping ${platform}`);
    continue;
  }
  platforms[platform] = {
    signature: readFileSync(join(dir, sigFile), "utf8").trim(),
    url: `https://github.com/HuakunShen/tunnel-deck/releases/download/${tag}/${archive}`,
  };
}

const keys = Object.keys(platforms);
if (keys.length === 0) {
  console.error("[updater] no signed platform archive found; refusing to write an empty feed");
  process.exit(1);
}
// A tagged release must feed every platform: a missing entry does not read
// as "no update", it strands those users on the old version with no error.
// Manual runs stay lenient (`--strict` off) for probing single platforms.
if (process.argv.includes("--strict")) {
  const missing = Object.values(PLATFORM)
    .map(([platform]) => platform)
    .filter((platform) => !keys.includes(platform));
  if (missing.length > 0) {
    console.error(`[updater] feed incomplete, missing: ${missing.join(", ")}`);
    process.exit(1);
  }
}
writeFileSync(
  out,
  `${JSON.stringify(
    {
      version,
      notes: `TunnelDeck ${version}`,
      pub_date: new Date().toISOString(),
      platforms,
    },
    null,
    2,
  )}\n`,
);
console.log(`[updater] ${out} (${keys.sort().join(", ")})`);
