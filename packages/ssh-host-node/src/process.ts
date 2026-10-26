/**
 * Node `ProcessPort`: spawns system `ssh` detached with stdout/stderr
 * appended to a per-tunnel log file. No shell is ever involved: argv passes
 * straight to `spawn`, eliminating quoting bugs and injection. `unref` lets
 * the daemon exit without killing tunnels; `killTree` reaps on demand.
 */
import { spawn } from "node:child_process";
import { openSync } from "node:fs";
import type { ProcessPort, SpawnedProcess } from "./ports.js";

export function createNodeProcessPort(sshPath = "ssh"): ProcessPort {
  return {
    spawn(argv, options): SpawnedProcess {
      let outFd: number | "ignore" = "ignore";
      let errFd: number | "ignore" = "ignore";
      try {
        outFd = openSync(options.logFile, "a");
        errFd = openSync(options.logFile, "a");
      } catch {
        outFd = "ignore";
        errFd = "ignore";
      }
      const child = spawn(sshPath, [...argv], {
        detached: true,
        // Windows would otherwise open a console window per tunnel.
        windowsHide: true,
        stdio: ["ignore", outFd, errFd],
      });
      const pid = child.pid ?? -1;
      const exited = new Promise<{ code: number | null; signal: string | null }>(
        (resolve) => {
          child.on("exit", (code, signal) => {
            resolve({ code, signal });
          });
          child.on("error", () => {
            resolve({ code: 127, signal: null });
          });
        },
      );
      child.unref();
      return {
        pid,
        exited,
        kill(signal = "SIGTERM"): void {
          try {
            if (child.pid !== undefined) {
              process.kill(child.pid, signal as NodeJS.Signals);
            }
          } catch {
            // Already gone.
          }
        },
      };
    },

    isAlive(pid): boolean {
      try {
        process.kill(pid, 0);
        return true;
      } catch {
        return false;
      }
    },

    killTree(pid): void {
      // `ssh -N` has no grandchildren; a single SIGTERM suffices, SIGKILL is
      // escalated by the manager after its grace window.
      try {
        process.kill(pid, "SIGTERM");
      } catch {
        // Already gone.
      }
    },
  };
}
