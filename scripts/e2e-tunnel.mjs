/**
 * End-to-end tunnel check against a real `sshd`, run by CI on every OS.
 *
 * Starts the built daemon, creates a profile whose local forward points back at
 * the daemon itself over SSH, starts it through the API, and fetches the
 * daemon's health endpoint *through the tunnel*. If the system `ssh` cannot be
 * spawned, the forward does not bind, or stop leaves the port open, this fails.
 *
 * Environment: E2E_SSH_PORT, E2E_SSH_USER, E2E_SSH_KEY (path to a private key
 * the sshd accepts).
 */
import { spawn } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const DAEMON = 7499;
const FORWARD = 18099;
const base = `http://127.0.0.1:${DAEMON}`;
const env = (name) => {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required`);
  return value;
};

const daemon = spawn(process.execPath, [join(root, "apps/server/dist/server.mjs"), `--port=${DAEMON}`], {
  stdio: "inherit",
});
let failed = false;
try {
  for (let i = 0; i < 30; i += 1) {
    const ready = await fetch(`${base}/api/health`).then((r) => r.ok, () => false);
    if (ready) break;
    await sleep(1000);
  }

  const created = await fetch(`${base}/api/profiles`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      name: "e2e",
      mode: "local",
      sshTarget: "127.0.0.1",
      sshPort: Number(env("E2E_SSH_PORT")),
      sshUser: env("E2E_SSH_USER"),
      identityFile: env("E2E_SSH_KEY"),
      forwards: [{ kind: "local", bindPort: FORWARD, targetHost: "127.0.0.1", targetPort: DAEMON }],
    }),
  });
  if (created.status !== 201) throw new Error(`create profile: ${created.status} ${await created.text()}`);
  const { id } = await created.json();

  const started = await (await fetch(`${base}/api/states/${id}/start`, { method: "POST" })).json();
  console.log("start ->", started.status, started.lastError ?? "");
  if (started.status !== "running" && started.status !== "degraded") {
    const log = await fetch(`${base}/api/profiles/${id}/log?tail=50`).then((r) => r.json(), () => ({}));
    console.log((log.lines ?? []).join("\n"));
    throw new Error(`tunnel did not come up: ${started.status}`);
  }

  // The proof: bytes crossed the SSH connection and came back.
  let through = false;
  for (let i = 0; i < 20 && !through; i += 1) {
    through = await fetch(`http://127.0.0.1:${FORWARD}/api/health`).then((r) => r.ok, () => false);
    if (!through) await sleep(500);
  }
  if (!through) throw new Error("health endpoint unreachable through the forwarded port");
  console.log("health through the tunnel: ok");

  const stopped = await (await fetch(`${base}/api/states/${id}/stop`, { method: "POST" })).json();
  console.log("stop ->", stopped.status);
  await sleep(500);
  const stillOpen = await fetch(`http://127.0.0.1:${FORWARD}/api/health`).then(() => true, () => false);
  if (stillOpen) throw new Error("the forwarded port is still open after stop");
  console.log("forwarded port closed after stop: ok");
} catch (error) {
  failed = true;
  console.error(String(error));
} finally {
  daemon.kill();
}
process.exit(failed ? 1 : 0);
