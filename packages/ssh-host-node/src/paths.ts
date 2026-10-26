/**
 * Where the user's files live, answered the same way on every platform.
 *
 * `os.homedir()` honours `HOME` on POSIX and `USERPROFILE` on Windows, which
 * is what OpenSSH itself uses to find `~/.ssh`. Reading `HOME` directly would
 * miss Windows entirely (or pick up a Git Bash `HOME` that OpenSSH ignores),
 * so every module resolves the home directory through this one function.
 */
import { homedir } from "node:os";
import { join } from "node:path";

/** The current user's home directory. */
export function userHome(): string {
  return homedir();
}

/** The daemon's own data directory (`profiles.json`, logs, control sockets). */
export function dataDir(home: string = userHome()): string {
  return join(home, ".ssh-tunnel-ui");
}
