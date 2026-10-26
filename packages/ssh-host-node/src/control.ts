/**
 * SSH control-socket port — the handle on a tunnel this daemon does not own.
 *
 * A tunnel handed to the user's own terminal is a child of *that* terminal, so
 * its pid says nothing to us and signalling it would be wrong. What it can
 * offer instead is a `ControlMaster` socket: `ssh -O check` says whether the
 * master is alive, and `ssh -O exit` shuts it down through the same connection
 * the forwards live on. Both are asked of OpenSSH itself rather than inferred
 * from a listening port, so the answer is about *this* tunnel, not about
 * whatever else happens to sit on the port.
 *
 * Nothing here runs a shell: argv goes straight to `spawn`, and the command is
 * bounded because a control socket that stopped answering must not hang a
 * status poll.
 */
import { spawn } from "node:child_process";
import { platform } from "node:os";
import type { ControlPort } from "./ports.js";

/** How long `ssh -O` may take before the socket is treated as gone. */
const CONTROL_TIMEOUT_MS = 2_000;

/**
 * Can this host's OpenSSH multiplex sessions at all?
 *
 * The Windows build of ssh has no `ControlMaster`: passing the option does not
 * create a socket, so nothing can be checked or ended through one. Callers ask
 * this before building a command line, so the same profile yields something
 * that actually works instead of a quietly inert option.
 */
export function supportsControlMaster(
  hostPlatform: NodeJS.Platform = platform(),
): boolean {
  return hostPlatform !== "win32";
}

function runControl(
  sshPath: string,
  action: "check" | "exit",
  socketPath: string,
  destination: string,
  timeoutMs: number,
): Promise<boolean> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (ok: boolean): void => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(ok);
    };
    const child = spawn(
      sshPath,
      ["-o", "BatchMode=yes", "-S", socketPath, "-O", action, destination],
      { stdio: "ignore", windowsHide: true },
    );
    const timer = setTimeout(() => {
      // A wedged socket: report "not there" rather than blocking the caller.
      child.kill("SIGKILL");
      finish(false);
    }, timeoutMs);
    child.once("error", () => finish(false));
    child.once("exit", (code) => finish(code === 0));
  });
}

export function createControlPort(
  sshPath = "ssh",
  timeoutMs = CONTROL_TIMEOUT_MS,
): ControlPort {
  return {
    check(socketPath, destination): Promise<boolean> {
      return runControl(sshPath, "check", socketPath, destination, timeoutMs);
    },
    exit(socketPath, destination): Promise<boolean> {
      return runControl(sshPath, "exit", socketPath, destination, timeoutMs);
    },
  };
}
