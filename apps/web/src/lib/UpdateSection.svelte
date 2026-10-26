<!--
  In-app updates, desktop only.

  Rendered inside Settings when the page runs in the Tauri shell. Everything
  loads through dynamic imports, so the browser/SSR build never resolves the
  Tauri JS bridge: outside the shell this component is not mounted at all
  (see SettingsSheet). No system dialog ever appears (`dialog: false` in
  tauri.conf.json) — an update must never modal over a running tunnel.
-->
<script lang="ts">
  import type { Update } from "@tauri-apps/plugin-updater";

  type Phase =
    | { kind: "idle" }
    | { kind: "checking" }
    | { kind: "uptodate" }
    | { kind: "available" }
    | { kind: "installing" }
    | { kind: "failed"; message: string };

  let version = $state<string | null>(null);
  let phase = $state<Phase>({ kind: "idle" });
  let pending = $state<Update | null>(null);

  // The shell version is known at mount; the feed is only touched when the
  // user asks, so opening Settings never phones home by itself.
  $effect(() => {
    void (async () => {
      try {
        const app = await import("@tauri-apps/api/app");
        version = await app.getVersion();
      } catch {
        version = null;
      }
    })();
  });

  async function checkNow(): Promise<void> {
    if (phase.kind === "checking" || phase.kind === "installing") return;
    phase = { kind: "checking" };
    pending = null;
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const found = await check();
      if (found === null) {
        phase = { kind: "uptodate" };
      } else {
        pending = found;
        phase = { kind: "available" };
      }
    } catch (error) {
      phase = {
        kind: "failed",
        message: error instanceof Error ? error.message : "update check failed",
      };
    }
  }

  async function installNow(): Promise<void> {
    if (pending === null || phase.kind === "installing") return;
    phase = { kind: "installing" };
    try {
      await pending.downloadAndInstall();
      const process = await import("@tauri-apps/plugin-process");
      await process.relaunch();
    } catch (error) {
      phase = {
        kind: "failed",
        message: error instanceof Error ? error.message : "install failed",
      };
    }
  }
</script>

<div class="flex flex-col gap-2 border-t border-border pt-4">
  <div class="flex items-baseline justify-between">
    <span class="text-[12px]">Updates</span>
    <span class="text-[11px] text-muted-foreground">
      {#if version === null}desktop app{:else}v{version}{/if}
    </span>
  </div>

  {#if phase.kind === "available" && pending !== null}
    <p class="text-[11px] leading-snug">
      v{pending.version} is ready{#if pending.body} — {pending.body.slice(0, 160)}{/if}
    </p>
    <button class="toolbar-button w-fit" onclick={() => void installNow()}>
      Install and relaunch
    </button>
  {:else if phase.kind === "installing"}
    <p class="text-[11px] text-muted-foreground">Installing — the app relaunches when it lands…</p>
  {:else if phase.kind === "checking"}
    <p class="text-[11px] text-muted-foreground">Checking for updates…</p>
  {:else if phase.kind === "uptodate"}
    <p class="text-[11px] text-muted-foreground">You are on the latest release.</p>
  {:else if phase.kind === "failed"}
    <p class="text-[11px] text-destructive">{phase.message}</p>
    <button class="toolbar-button w-fit" onclick={() => void checkNow()}>Try again</button>
  {:else}
    <button class="toolbar-button w-fit" onclick={() => void checkNow()}>Check for updates</button>
  {/if}
</div>
