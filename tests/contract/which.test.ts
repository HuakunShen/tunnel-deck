/**
 * Executable lookup is where the host stops being an abstraction: Windows
 * separates `PATH` with `;` and stores `node.exe`, so the POSIX-shaped lookup
 * that worked on macOS silently finds nothing there — and the daemon then
 * "fails to start" for a reason the user cannot see.
 */
import { afterEach, describe, expect, it } from "vitest";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { executableNames, whichOnPath } from "@ssh-tunnel/host-node";

const created: string[] = [];

function tempDir(): string {
  const dir = mkdtempSync(join(tmpdir(), "ssh-tunnel-which-"));
  created.push(dir);
  return dir;
}

afterEach(() => {
  for (const dir of created.splice(0)) rmSync(dir, { recursive: true, force: true });
});

describe("executableNames", () => {
  it("tries only the bare name on POSIX", () => {
    expect(executableNames("node", "darwin")).toEqual(["node"]);
    expect(executableNames("node", "linux")).toEqual(["node"]);
  });

  it("appends PATHEXT suffixes on Windows", () => {
    expect(executableNames("node", "win32", ".COM;.EXE;.BAT;.CMD")).toEqual([
      "node.com",
      "node.exe",
      "node.bat",
      "node.cmd",
    ]);
    // A name that already carries an extension is tried as written.
    expect(executableNames("wt.exe", "win32", ".EXE")).toEqual(["wt.exe"]);
  });
});

describe("whichOnPath", () => {
  it("finds an executable in a later PATH entry", () => {
    const empty = tempDir();
    const dir = tempDir();
    // Host-shaped on purpose: a Windows temp path holds `C:`, which a `:`
    // separator would split, and its executables carry an extension.
    const name = process.platform === "win32" ? "node.exe" : "node";
    writeFileSync(join(dir, name), "#!/bin/sh\n", { mode: 0o755 });
    expect(
      whichOnPath("node", { platform: process.platform, pathValue: [empty, dir].join(delimiter) }),
    ).toBe(join(dir, name));
  });

  it("splits Windows PATH on semicolons and looks for node.exe", () => {
    const empty = tempDir();
    const dir = tempDir();
    // Written executable because this test runs on POSIX, where `X_OK` is a
    // real permission check; on Windows existence is the whole signal.
    writeFileSync(join(dir, "node.exe"), "MZ", { mode: 0o755 });
    expect(
      whichOnPath("node", {
        platform: "win32",
        pathValue: `${empty};${dir}`,
        pathExtensions: ".EXE",
      }),
    ).toBe(join(dir, "node.exe"));
  });

  it("ignores empty PATH entries and directories named like the command", () => {
    const dir = tempDir();
    mkdirSync(join(dir, "node"));
    expect(whichOnPath("node", { platform: "darwin", pathValue: `:${dir}:` })).toBeUndefined();
  });

  it("returns undefined when nothing matches", () => {
    const dir = tempDir();
    expect(whichOnPath("definitely-not-here", { platform: "darwin", pathValue: dir })).toBeUndefined();
  });
});
