/**
 * Terminal discovery + launch — the "Open in…" affordance.
 *
 * The daemon spawns the terminal *itself* and keeps the pid, so an external
 * session stays manageable (Close ends it) even though the user may also close
 * the window freely. Health never depends on the terminal process: it is
 * measured by probing the forwarded ports, so a hand-closed window is a clean,
 * accurate state rather than a leak.
 *
 * Only terminals whose CLI accepts "run this one command" are offered, and
 * discovery is pure filesystem probing — nothing here runs a candidate.
 *
 * Launch is where the OS can quietly refuse: AppleScript automation denied,
 * a bundle that moved, a launcher that is not installed after all. The spawn is
 * therefore observed for a short settle window, and a launcher that dies with
 * a non-zero status is reported with its own stderr instead of a pid that
 * means nothing.
 */
import { spawn } from "node:child_process";
import {
  accessSync,
  closeSync,
  constants,
  openSync,
  readFileSync,
  readdirSync,
  unlinkSync,
} from "node:fs";
import { join } from "node:path";
import { homedir, platform, tmpdir } from "node:os";
import type { LaunchMode } from "@ssh-tunnel/contract";
import type { TerminalPort, TerminalTargetInfo } from "./ports.js";
import { whichOnPath } from "./which.js";

/** A terminal the daemon knows how to launch with a single command. */
interface TerminalTarget extends TerminalTargetInfo {
  /** Executable to spawn; resolved from PATH unless `absolutePath` is set. */
  readonly command: string;
  readonly absolutePath?: string;
  /** Build argv for launching `commandLine` (already display-quoted). */
  argvFor(commandLine: string): readonly string[];
}

function canRead(path: string): boolean {
  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}



/** Directories macOS keeps app bundles in, in the order Finder lists them. */
function applicationDirectories(): readonly string[] {
  return [
    "/Applications",
    "/System/Applications",
    "/System/Applications/Utilities",
    join(homedir(), "Applications"),
  ];
}

/**
 * Resolve a bundle to its launcher executable. The binary inside a bundle is
 * not required to match the app name, so pick the one that exists and is
 * executable rather than assuming.
 */
function bundleExecutable(app: string): string | undefined {
  for (const dir of applicationDirectories()) {
    const macos = join(dir, `${app}.app`, "Contents", "MacOS");
    let entries: string[];
    try {
      entries = readdirSync(macos);
    } catch {
      continue;
    }
    const binary = entries.find((name) => !name.endsWith(".app") && canRead(join(macos, name)));
    if (binary !== undefined) return join(macos, binary);
  }
  return undefined;
}

/** Quote a string for an AppleScript literal (backslash and double quote). */
function appleScriptString(value: string): string {
  return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

/** POSIX single-quoting for the shell literal we build inside AppleScript. */
function shellSingleQuoted(value: string): string {
  return `'${value.replace(/'/g, "'\\''")}'`;
}

const macTargets: readonly TerminalTarget[] = [
  {
    id: "terminal",
    label: "Terminal",
    hint: "com.apple.Terminal",
    platform: "darwin",
    command: "osascript",
    // `open -a Terminal <argv>` is ignored by Terminal.app; AppleScript's
    // `do script` is the documented way to run a command in a new tab.
    argvFor: (cmd) => [
      "-e",
      `tell application "Terminal" to do script ${appleScriptString(`cd ${shellSingleQuoted("~")}\n${cmd}`)}`,
    ],
  },
  {
    id: "iterm2",
    label: "iTerm2",
    hint: "com.googlecode.iterm2",
    platform: "darwin",
    command: "osascript",
    argvFor: (cmd) => [
      "-e",
      `tell application "iTerm" to create window with default profile command ${appleScriptString(cmd)}`,
    ],
  },
  {
    id: "warp",
    label: "Warp",
    hint: "dev.warp.Warp-Stable",
    platform: "darwin",
    command: "open",
    argvFor: (cmd) => ["-a", "Warp", "--", "/bin/zsh", "-lc", cmd],
  },
  {
    id: "ghostty",
    label: "Ghostty",
    hint: "com.mitchellh.ghostty",
    platform: "darwin",
    command: "open",
    argvFor: (cmd) => ["-a", "Ghostty", "--", "/bin/zsh", "-lc", cmd],
  },
  {
    id: "kitty",
    label: "kitty",
    hint: "kitty",
    platform: "darwin",
    command: "kitty",
    argvFor: (cmd) => ["--", "/bin/zsh", "-lc", cmd],
  },
  {
    id: "wezterm",
    label: "WezTerm",
    hint: "com.github.wez.wezterm",
    platform: "darwin",
    command: "wezterm",
    argvFor: (cmd) => ["start", "--", "/bin/zsh", "-lc", cmd],
  },
  {
    id: "alacritty",
    label: "Alacritty",
    hint: "org.alacritty",
    platform: "darwin",
    command: "alacritty",
    argvFor: (cmd) => ["--", "/bin/zsh", "-lc", cmd],
  },
  {
    id: "vscode",
    label: "VS Code",
    hint: "com.microsoft.VSCode",
    platform: "darwin",
    command: "code",
    argvFor: (cmd) => ["--new-window", "/bin/zsh", "-lc", cmd],
  },
];

const linuxTargets: readonly TerminalTarget[] = [
  {
    id: "gnome-terminal",
    label: "GNOME Terminal",
    hint: "gnome-terminal",
    platform: "linux",
    command: "gnome-terminal",
    argvFor: (cmd) => ["--", "/bin/sh", "-lc", cmd],
  },
  {
    id: "konsole",
    label: "Konsole",
    hint: "org.kde.konsole",
    platform: "linux",
    command: "konsole",
    argvFor: (cmd) => ["-e", "/bin/sh", "-lc", cmd],
  },
  {
    id: "alacritty",
    label: "Alacritty",
    hint: "org.alacritty",
    platform: "linux",
    command: "alacritty",
    argvFor: (cmd) => ["--", "/bin/sh", "-lc", cmd],
  },
  {
    id: "kitty",
    label: "kitty",
    hint: "kitty",
    platform: "linux",
    command: "kitty",
    argvFor: (cmd) => ["--", "/bin/sh", "-lc", cmd],
  },
  {
    id: "xterm",
    label: "XTerm",
    hint: "xterm",
    platform: "linux",
    command: "xterm",
    argvFor: (cmd) => ["-e", "/bin/sh", "-lc", cmd],
  },
];

const windowsTargets: readonly TerminalTarget[] = [
  {
    id: "windows-terminal",
    label: "Windows Terminal",
    hint: "wt.exe",
    platform: "win32",
    command: "wt.exe",
    argvFor: (cmd) => ["new-tab", "--title", "TunnelDeck", "cmd.exe", "/c", cmd],
  },
  {
    id: "powershell",
    label: "PowerShell",
    hint: "powershell.exe",
    platform: "win32",
    command: "powershell.exe",
    argvFor: (cmd) => ["-NoExit", "-Command", cmd],
  },
];

/**
 * Bundle names proving installation. A terminal may ship no PATH shim at all
 * (Warp), and the executable inside a bundle does not have to match the app
 * name (iTerm.app contains `iTerm2`), so detection looks for the bundle and
 * never guesses the binary.
 */
const macBundleProbe: Readonly<Record<string, readonly string[]>> = {
  terminal: ["Terminal"],
  iterm2: ["iTerm", "iTerm2"],
  warp: ["Warp", "WarpPreview"],
  ghostty: ["Ghostty"],
  kitty: ["kitty"],
  wezterm: ["WezTerm"],
  alacritty: ["Alacritty"],
  vscode: ["Visual Studio Code"],
};

/**
 * Detect installed terminals for `hostPlatform`. macOS resolves the `.app`
 * bundle; Linux and Windows probe the executable on PATH.
 */
export function detectTerminals(
  hostPlatform: NodeJS.Platform = platform(),
): readonly TerminalTarget[] {
  const table =
    hostPlatform === "darwin"
      ? macTargets
      : hostPlatform === "win32"
        ? windowsTargets
        : linuxTargets;

  const found: TerminalTarget[] = [];
  for (const target of table) {
    let absolutePath: string | undefined;
    if (hostPlatform === "darwin") {
      for (const app of macBundleProbe[target.id] ?? []) {
        absolutePath = bundleExecutable(app);
        if (absolutePath !== undefined) break;
      }
      // The bundle is the installation signal: PATH shims are absent for most
      // of these apps, and `osascript`/`open` launchers must not count.
      if (absolutePath === undefined) continue;
    } else if (whichOnPath(target.command) === undefined) {
      continue;
    }
    found.push(
      absolutePath === undefined ? target : { ...target, absolutePath },
    );
  }
  return found;
}

/**
 * A launchable session the daemon owns: the terminal process it spawned, so it
 * can be ended on demand. Kept distinct from the managed `ssh` child so we
 * never claim to supervise a GUI app beyond its launcher pid.
 */
export interface ExternalSession {
  readonly profileId: string;
  readonly terminalId: string;
  /** `tunnel` runs `ssh -N`; `shell` leaves the session at a prompt. */
  readonly mode: LaunchMode;
  /** Keyed as `command` so it matches the wire contract directly. */
  readonly command: string;
  readonly pid: number;
  readonly launchedAt: string;
}

/**
 * How long a launcher gets to fail.
 *
 * `open` and `osascript` finish in milliseconds, and a non-zero exit is a real
 * error worth reporting (iTerm2 refusing Apple events, a missing binary). A
 * launcher still alive after this window *is* the terminal — kitty, xterm,
 * Windows Terminal — and must keep running: it holds the tunnel's window.
 */
const LAUNCH_SETTLE_MS = 700;

/** How much of a launcher's stderr travels into the error message. */
const LAUNCH_STDERR_LIMIT = 2_000;

/** Everything the launcher wrote to stderr, already trimmed and bounded. */
function readLauncherLog(path: string): string {
  try {
    return readFileSync(path, "utf8").slice(0, LAUNCH_STDERR_LIMIT).trim();
  } catch {
    return "";
  }
}

export function createTerminalPort(
  hostPlatform: NodeJS.Platform = platform(),
): TerminalPort {
  const targets = detectTerminals(hostPlatform);

  const find = (terminalId: string): TerminalTarget => {
    const target = targets.find((candidate) => candidate.id === terminalId);
    if (target === undefined) {
      throw new Error(`terminal ${terminalId} is not installed on this machine`);
    }
    return target;
  };

  return {
    detect(): readonly TerminalTargetInfo[] {
      return targets.map(({ id, label, hint, platform: p }) => ({
        id,
        label,
        hint,
        platform: p,
      }));
    },

    async launch(terminalId: string, commandLine: string): Promise<number> {
      const target = find(terminalId);
      // stderr goes to a file rather than a pipe: the pipe would keep the
      // daemon's event loop referenced for as long as the terminal lives, and
      // we only ever read it back when the launcher died.
      const logPath = join(
        tmpdir(),
        `ssh-tunnel-launch-${process.pid}-${Date.now()}.log`,
      );
      let logFd: number | "ignore" = "ignore";
      try {
        logFd = openSync(logPath, "a");
      } catch {
        logFd = "ignore";
      }
      const child = spawn(target.command, [...target.argvFor(commandLine)], {
        detached: true,
        stdio: ["ignore", "ignore", logFd],
      });
      if (typeof logFd === "number") closeSync(logFd);

      const outcome = await new Promise<
        { readonly kind: "alive" } | { readonly kind: "exit"; readonly code: number | null }
      >((resolve) => {
        const settle = setTimeout(() => resolve({ kind: "alive" }), LAUNCH_SETTLE_MS);
        child.once("error", () => {
          clearTimeout(settle);
          resolve({ kind: "exit", code: 127 });
        });
        child.once("exit", (code) => {
          clearTimeout(settle);
          resolve({ kind: "exit", code });
        });
      });

      const detail = outcome.kind === "exit" ? readLauncherLog(logPath) : "";
      try {
        unlinkSync(logPath);
      } catch {
        // The terminal may still hold it open (Windows); a temp file is cheap.
      }

      if (outcome.kind === "exit" && outcome.code !== 0) {
        const reason =
          detail.length > 0
            ? detail.split("\n").slice(-3).join(" ")
            : `exit ${outcome.code ?? "unknown"}`;
        throw new Error(`${target.label} refused the launch: ${reason}`);
      }

      child.unref();
      return child.pid ?? -1;
    },

    close(pid: number): void {
      try {
        process.kill(pid, "SIGTERM");
      } catch {
        // Already gone — the user may have closed the window themselves.
      }
    },
  };
}