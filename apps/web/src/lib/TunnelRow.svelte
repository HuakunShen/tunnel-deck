<!--
  TunnelRow: one tunnel in the console list. Reads as a macOS list row (flat,
  hairline separated) rather than a card, with the actions a network operator
  actually wants: start/stop, probe, copy the exact ssh command, and "Open in"
  a real terminal window when the host reports one.
-->
<script lang="ts">
  import {
    CircleStop,
    Copy,
    Check,
    ExternalLink,
    FlaskConical,
    Link,
    LoaderCircle,
    Pencil,
    Play,
    ScrollText,
    Terminal as TerminalIcon,
    Trash,
  } from "@lucide/svelte";
  import { endpointSummary, localForwardEndpoint, type LocalEndpoint } from "@ssh-tunnel/core";
  import {
    openUrl,
    type LaunchMode,
    type TerminalTarget,
    type TunnelProfile,
    type TunnelState,
  } from "$lib/store";

  interface Props {
    profile: TunnelProfile;
    tunnelState?: TunnelState;
    busy: boolean;
    terminals: readonly TerminalTarget[];
    onStart: (id: string) => void;
    onStop: (id: string) => void;
    onProbe: (id: string) => void;
    onCopy: (id: string) => void;
    onLaunch: (id: string, terminalId: string, mode: LaunchMode) => void;
    onLog: (id: string) => void;
    onEdit: (profile: TunnelProfile) => void;
    onDelete: (id: string, name: string) => void;
  }

  let {
    profile,
    tunnelState,
    busy,
    terminals,
    onStart,
    onStop,
    onProbe,
    onCopy,
    onLaunch,
    onLog,
    onEdit,
    onDelete,
  }: Props = $props();

  let menuOpen = $state(false);
  let copied = $state(false);
  let endpointCopied = $state(false);
  let openError = $state<string | undefined>(undefined);

  /**
   * Each forward paired with the endpoint a browser can open, when it has one.
   * Only `ssh -L` listens on this machine, so SOCKS5 and `-R` pair with nothing
   * and stay plain text — offering a link there would be a dead address.
   */
  const rows = $derived(
    profile.forwards.map((forward) => ({
      forward,
      endpoint: localForwardEndpoint(forward),
      text: forwardText(forward),
    })),
  );

  // One pasteable line of every browsable endpoint in this profile.
  const pasteable = $derived(
    endpointSummary(
      rows.map((row) => row.endpoint).filter((e): e is LocalEndpoint => e !== undefined),
    ),
  );

  const status = $derived(tunnelState?.status ?? "stopped");
  const live = $derived(status === "running" || status === "degraded" || status === "starting");

  const dotClass = $derived(
    status === "running"
      ? "bg-signal-running"
      : status === "degraded"
        ? "bg-signal-degraded"
        : status === "failed"
          ? "bg-signal-failed"
          : status === "starting" || status === "stopping"
            ? "bg-signal-starting animate-pulse"
            : "bg-signal-idle",
  );

  // Human wording, not `-L/-R/-D`: the mode name explains itself.
  const modeLabel = $derived(
    profile.mode === "local"
      ? "Local"
      : profile.mode === "remote"
        ? "Remote"
        : profile.mode === "dynamic"
          ? "Dynamic"
          : profile.mode === "jump"
            ? "Jump"
            : "Multi",
  );

  function forwardText(f: TunnelProfile["forwards"][number]): string {
    if (f.kind === "dynamic") {
      return `SOCKS5 on ${(f.bindAddress ? `${f.bindAddress}:` : "") + f.bindPort}`;
    }
    const bind = (f.bindAddress ? `${f.bindAddress}:` : "") + String(f.bindPort);
    const arrow = f.kind === "local" ? "→" : "←";
    return `${localForwardEndpoint(f)?.endpoint ?? "localhost:" + bind} ${arrow} ${f.targetHost}:${f.targetPort}`;
  }

  /**
   * Hand the address to the OS. Opening it in this window instead would take
   * the console away from the user — an anchor navigates the webview frame.
   */
  async function browse(endpoint: LocalEndpoint): Promise<void> {
    openError = undefined;
    try {
      await openUrl(endpoint.url);
    } catch (error) {
      openError = error instanceof Error ? error.message : "could not open the browser";
    }
  }

  async function copyEndpoint(): Promise<void> {
    if (pasteable === "") return;
    await navigator.clipboard.writeText(pasteable);
    endpointCopied = true;
    setTimeout(() => (endpointCopied = false), 1600);
  }

  function pick(terminalId: string, mode: LaunchMode): void {
    menuOpen = false;
    onLaunch(profile.id, terminalId, mode);
  }

  async function copy(): Promise<void> {
    onCopy(profile.id);
    copied = true;
    setTimeout(() => (copied = false), 1600);
  }
</script>

<svelte:window onclick={() => (menuOpen = false)} />

<div class="tunnel-row">
  <span class={`h-2 w-2 shrink-0 rounded-full ${dotClass}`} title={status}></span>

  <div class="min-w-0 flex-1">
    <div class="flex items-center gap-8">
      <span class="row-name">{profile.name}</span>
      <span class="row-detail">{modeLabel}</span>
      <span class="row-detail capitalize">{status}</span>
      {#if tunnelState?.origin === "terminal" && live}
        <span class="row-detail" title="Opened in a terminal window; state comes from its SSH control socket">
          terminal
        </span>
      {:else if tunnelState?.origin === "external" && live}
        <span class="row-detail" title="Not started by this app; state comes from the bind port">
          external
        </span>
      {/if}
    </div>
    <div class="row-detail mt-[3px] flex flex-wrap items-center gap-x-3 font-mono">
      <span class="truncate">
        {profile.sshTarget}{#if profile.jumpHops?.length}&nbsp;via&nbsp;{profile.jumpHops.join(", ")}{/if}
      </span>
      {#each rows as row (row.text)}
        {#if row.endpoint}
          <button
            class="endpoint truncate"
            type="button"
            onclick={() => void browse(row.endpoint as LocalEndpoint)}
            title="Open {row.endpoint.url} in your browser. Copy {row.endpoint.endpoint} to paste into a terminal, a browser bar, or a database client."
          >
            <ExternalLink size={11} />
            {row.endpoint.endpoint}
          </button>
        {:else}
          <span class="truncate">{row.text}</span>
        {/if}
      {/each}
    </div>
    {#if openError !== undefined}
      <div class="mt-1 flex items-center gap-2">
        <p class="truncate text-[10px] text-destructive" title={openError}>{openError}</p>
        <button class="shrink-0 text-[10px] text-muted-foreground underline hover:text-foreground" onclick={() => onLog(profile.id)}>
          log
        </button>
      </div>
    {:else if tunnelState?.lastError}
      <div class="mt-1 flex items-center gap-2">
        <p class="truncate text-[10px] text-destructive" title={tunnelState.lastError}>{tunnelState.lastError}</p>
        <button class="shrink-0 text-[10px] text-muted-foreground underline hover:text-foreground" onclick={() => onLog(profile.id)}>
          log
        </button>
      </div>
    {/if}
  </div>

  <div class="row-actions shrink-0">
    {#if status === "starting" || status === "stopping"}
      {#key status}
        <button class="toolbar-button" disabled aria-busy="true" title="Working…">
          <LoaderCircle size={13} class="animate-spin" />
          {status === "starting" ? "Starting…" : "Stopping…"}
        </button>
      {/key}
    {:else if live}
      <button class="toolbar-button" disabled={busy} onclick={() => onStop(profile.id)} title="Stop tunnel">
        <CircleStop size={13} /> Stop
      </button>
    {:else}
      <button class="toolbar-button" disabled={busy} onclick={() => onStart(profile.id)} title="Start tunnel">
        <Play size={13} /> Start
      </button>
    {/if}

    <button class="toolbar-icon" onclick={() => void copy()} title="Copy the ssh command" aria-label="Copy the ssh command">
      {#if copied}<Check size={14} />{:else}<Copy size={14} />{/if}
    </button>

    <!--
      Every row renders the same controls in the same order. A control with
      nothing to act on stays in place but disabled, rather than disappearing:
      removing it would slide every icon to its left out of alignment with the
      rows above and below, which reads as a rendering fault.
    -->
    <button
      class="toolbar-icon"
      disabled={pasteable === ""}
      onclick={() => void copyEndpoint()}
      title={pasteable === ""
        ? "This tunnel has no local address to copy"
        : `Copy ${pasteable} — paste it into a terminal, a browser bar, or a database client`}
      aria-label="Copy the local address"
    >
      {#if endpointCopied}<Check size={14} />{:else}<Link size={14} />{/if}
    </button>

    <button class="toolbar-icon" disabled={busy} aria-busy={busy} onclick={() => onProbe(profile.id)} title="Probe forwarded ports" aria-label="Probe forwarded ports">
      {#if busy}<LoaderCircle size={14} class="animate-spin" />{:else}<FlaskConical size={14} />{/if}
    </button>

    <div class="relative">
      <button
        class="toolbar-icon"
        disabled={terminals.length === 0}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        onclick={(e) => { e.stopPropagation(); menuOpen = !menuOpen; }}
        title={terminals.length === 0 ? "No terminal applications found" : "Open in a terminal"}
        aria-label="Open in a terminal"
      >
        <TerminalIcon size={14} />
      </button>
      {#if menuOpen && terminals.length > 0}
          <div
            class="absolute right-0 z-20 mt-1 min-w-[184px] overflow-hidden rounded-[var(--radius)] border border-border bg-popover py-1 shadow-[0_8px_24px_rgb(0_0_0/0.18)]"
            role="menu"
            tabindex="-1"
            onkeydown={(e) => { if (e.key === "Escape") menuOpen = false; }}
          >
            <p class="px-3 py-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
              Tunnel only
            </p>
            {#each terminals as terminal (terminal.id)}
              <button
                class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[12px] hover:bg-accent"
                role="menuitem"
                onclick={() => pick(terminal.id, "tunnel")}
              >
                <ExternalLink size={12} class="text-muted-foreground" />
                <span class="flex-1 truncate">{terminal.label}</span>
                <span class="font-mono text-[10px] text-muted-foreground">-N</span>
              </button>
            {/each}
            <p
              class="mt-1 border-t border-border/60 px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground"
              title="The forwards live and die with the shell session"
            >
              Interactive shell
            </p>
            {#each terminals as terminal (terminal.id)}
              <button
                class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[12px] hover:bg-accent"
                role="menuitem"
                onclick={() => pick(terminal.id, "shell")}
              >
                <TerminalIcon size={12} class="text-muted-foreground" />
                <span class="flex-1 truncate">{terminal.label}</span>
                <span class="font-mono text-[10px] text-muted-foreground">shell</span>
              </button>
            {/each}
        </div>
      {/if}
    </div>

    <button
      class="toolbar-icon"
      onclick={() => onLog(profile.id)}
      title="View this tunnel's log"
      aria-label="View log"
    >
      <ScrollText size={14} />
    </button>

    <button class="toolbar-icon" onclick={() => onEdit(profile)} title="Edit" aria-label="Edit tunnel">
      <Pencil size={14} />
    </button>
    <button
      class="toolbar-icon hover:!text-destructive"
      onclick={() => onDelete(profile.id, profile.name)}
      title="Delete"
      aria-label="Delete tunnel"
    >
      <Trash size={14} />
    </button>
  </div>
</div>