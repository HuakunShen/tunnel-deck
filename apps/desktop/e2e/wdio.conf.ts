/**
 * WebdriverIO config for the TunnelDeck desktop app (Tauri 2).
 *
 * Follows the official Tauri WebDriver guide: the `@wdio/tauri-service`
 * runs an **embedded** WebDriver server inside the app itself, so macOS
 * needs no external driver. `appBinaryPath` points at the release bundle
 * binary built by `pnpm --filter @ssh-tunnel/desktop build:app`.
 *
 * Run: `pnpm --filter @ssh-tunnel/desktop test:e2e` (builds first).
 */
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const desktopDir = join(dirname(fileURLToPath(import.meta.url)), "..");
// Cargo can be redirected off the source tree (a scratch target dir keeps the
// checkout clean and avoids rebuilding the world). Point the driver at whichever
// one the build actually used.
const targetDir = process.env.CARGO_TARGET_DIR ?? join(desktopDir, "src-tauri/target");

// The binary only registers its embedded WebDriver when this is set, and the
// service passes its own environment to the app it launches. Setting it here
// means a bare `wdio run` behaves like `pnpm test:e2e` instead of hanging on a
// server this build was never configured to expose.
process.env.SSH_TUNNEL_WEBDRIVER ??= "1";

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: [join(desktopDir, "e2e/specs/**/*.js")],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      "tauri:options": {
        application: join(
          targetDir,
          "release/bundle/macos/TunnelDeck.app/Contents/MacOS/ssh-tunnel-desktop",
        ),
      },
    },
  ],
  services: [
    [
      "tauri",
      {
        driverProvider: "embedded",
      },
    ],
  ],
  reporters: ["spec"],
  framework: "mocha",
  mochaOpts: {
    ui: "bdd",
    timeout: 120000,
  },
};
