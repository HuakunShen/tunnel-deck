#!/usr/bin/env node
/**
 * Collect one platform's Tauri bundles into `dist/` with a name that says what
 * it is.
 *
 * Tauri names artifacts after the product in the native installer's own style
 * (`TunnelDeck_0.1.3_aarch64.dmg`, `TunnelDeck_0.1.3_x64-setup.exe`), which
 * leaves a download page where every file has a different shape and nothing
 * names the platform. This copies them to
 * `TunnelDeck-<version>-<label><suffix>` so the release assets read as one set.
 *
 * Usage: `node scripts/collect-bundles.mjs --label macos-arm64 --version 0.1.3 [--target aarch64-apple-darwin] [--out dist]`
 */
import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { basename, dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

/** Artifact extension → the suffix it gets in its release name. */
const SUFFIX = {
  ".dmg": "",
  ".deb": "",
  ".appimage": "",
  ".msi": "",
  // NSIS writes an installer, not the app, so say so.
  ".exe": "-setup",
};

/**
 * Updater payloads, matched by filename tail. Only macOS wraps its bundle
 * (`.app.tar.gz`); the v2 updater installs AppImage, deb, nsis and msi
 * directly, so those entries ARE the installer files (byte-identical to the
 * friendly downloads — copied twice under two names would be waste, see
 * below). Renaming is safe: the `.sig` signs the file bytes, never the name,
 * and `latest.json` points at the renamed files.
 *
 * Each entry maps a filename tail to the release-name suffix. Single-file
 * payloads whose friendly name already matches (`-setup.exe`, `.AppImage`)
 * are collected once, under the friendly name, and the feed points there.
 */
const UPDATER = [
  [".app.tar.gz", ".app.tar.gz"],
  [".AppImage", ".AppImage"],
  ["-setup.exe", "-setup.exe"],
];

function updaterName(file) {
  for (const [tail, suffix] of UPDATER) {
    if (file.endsWith(tail)) return suffix;
  }
  if (file.endsWith(".sig")) {
    const withoutSig = file.slice(0, -".sig".length);
    for (const [tail, suffix] of UPDATER) {
      if (withoutSig.endsWith(tail)) return `${suffix}.sig`;
    }
  }
  return undefined;
}

function arg(name, fallback) {
  const index = process.argv.indexOf(`--${name}`);
  if (index === -1 || process.argv[index + 1] === undefined) return fallback;
  return process.argv[index + 1];
}

const label = arg("label");
const version = arg("version");
if (label === undefined || version === undefined) {
  console.error("usage: collect-bundles.mjs --label <label> --version <semver> [--target <triple>] [--out <dir>]");
  process.exit(2);
}
// An empty `--target ""` (the matrix fills it for hosts that need none) means
// the plain `release/` tree, not a triple named "".
const target = arg("target", "") === "" ? undefined : arg("target");
const outDir = resolve(root, arg("out", "dist"));
// `--target` moves the whole build tree under the triple; without it the
// bundles sit directly in `release/`.
const bundleRoot = join(
  root,
  "apps",
  "desktop",
  "src-tauri",
  "target",
  ...(target === undefined ? [] : [target]),
  "release",
  "bundle",
);

if (!existsSync(bundleRoot)) {
  console.error(`[bundles] no bundle directory at ${bundleRoot}`);
  process.exit(1);
}

function walk(dir, found = []) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      walk(path, found);
      continue;
    }
    found.push(path);
  }
  return found;
}

mkdirSync(outDir, { recursive: true });
const copied = [];
for (const path of walk(bundleRoot)) {
  const file = basename(path);
  const updateSuffix = updaterName(file);
  if (updateSuffix !== undefined) {
    // The updater payload rides along only when the build signed it: without
    // the key there is a binary but no `.sig`, and an unsigned update feed
    // is worse than none. (The friendly download below is unaffected — it
    // never needs a signature.)
    const sig = updateSuffix.endsWith(".sig") ? path : `${path}.sig`;
    if (existsSync(sig)) {
      const name = `TunnelDeck-${version}-${label}${updateSuffix}`;
      copyFileSync(path, join(outDir, name));
      copied.push(name);
    }
  }
  const suffix = SUFFIX[extname(path).toLowerCase()];
  if (suffix === undefined) continue;
  const name = `TunnelDeck-${version}-${label}${suffix}${extname(path)}`;
  copyFileSync(path, join(outDir, name));
  copied.push(name);
}

if (copied.length === 0) {
  console.error(`[bundles] nothing to collect in ${bundleRoot} (looked for ${Object.keys(SUFFIX).join(", ")})`);
  process.exit(1);
}
for (const name of copied) console.log(`[bundles] ${basename(name)}`);
