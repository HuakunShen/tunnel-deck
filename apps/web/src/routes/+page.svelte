<!--
  Tunnel console shell: toolbar / sidebar / content / status bar, the same
  frame space-lens uses so this app reads as a native desktop window rather
  than a centered web page. Polls the daemon every 3s; all mutations go
  through the shared typed client.
-->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    CircleStop,
    Plus,
    RefreshCw,
    Settings as SettingsIcon,
    SquareTerminal,
    Wifi,
    WifiOff,
  } from "@lucide/svelte";
  import TunnelRow from "$lib/TunnelRow.svelte";
  import LogSheet from "$lib/LogSheet.svelte";
  import SettingsSheet from "$lib/SettingsSheet.svelte";
  import TunnelEditor from "$lib/TunnelEditor.svelte";
  import type { TunnelStatus } from "@ssh-tunnel/contract";
  import {
    getClient,
    initialStore,
    refreshAll,
    refreshHosts,
    refreshTerminals,
    type LaunchMode,
    type TerminalTarget,
    type TunnelProfile,
    type TunnelProfileInput,
  } from "$lib/store";

  let store = $state({ ...initialStore });
  let editorOpen = $state(false);
  let logOpen = $state(false);
  let logProfile = $state<TunnelProfile | null>(null);
  let settingsOpen = $state(false);
  // One knob for the whole window: the daemon has no opinion about appearance,
  // so this is a page preference kept in localStorage.
  const OPACITY_KEY = "ssh-tunnel.material-opacity";
  const defaultOpacity = 0.72;
  let materialOpacity = $state(defaultOpacity);
  // Only true once the desktop host reports it really installed a material.
  let hasMaterial = $state(false);

  function readOpacity(): number {
    try {
      const stored = Number(window.localStorage.getItem(OPACITY_KEY));
      return Number.isFinite(stored) && stored >= 0.4 && stored <= 0.94 ? stored : defaultOpacity;
    } catch {
      // Private browsing can refuse storage; the default still applies.
      return defaultOpacity;
    }
  }

  function applyOpacity(value: number): void {
    materialOpacity = value;
    document.documentElement.style.setProperty("--material-opacity", String(value));
    try {
      window.localStorage.setItem(OPACITY_KEY, String(value));
    } catch {
      // Not being able to remember it is not worth interrupting anyone over.
    }
  }

  onMount(() => {
    applyOpacity(readOpacity());
    const host = window.__nativeHost;
    if (host !== undefined) {
      hasMaterial = document.documentElement.dataset.backdrop === "glass"
        || document.documentElement.dataset.backdrop === "vibrancy";
      window.addEventListener("nativehost", () => {
        const value = document.documentElement.dataset.backdrop;
        hasMaterial = value === "glass" || value === "vibrancy";
      });
    }
  });
  let editing: TunnelProfile | null = $state(null);
  let saving = $state(false);
  let editorError: string | null = $state(null);
  let busyIds = $state(new Set<string>());
  let toast = $state<string | null>(null);
  let timer: ReturnType<typeof setInterval> | undefined = undefined;
  let toastTimer: ReturnType<typeof setTimeout> | undefined = undefined;

  // Sidebar filter: all, running, or stopped. Pure view state — the daemon
  // still owns every profile.
  let filter = $state<"all" | "live" | "idle">("all");

  const running = $derived(store.states.filter((s) => s.status === "running").length);
  const failed = $derived(store.states.filter((s) => s.status === "failed").length);

  const visible = $derived(
    store.profiles.filter((profile) => {
      if (filter === "all") return true;
      const status = store.states.find((s) => s.profileId === profile.id)?.status ?? "stopped";
      const isLive = status === "running" || status === "starting" || status === "degraded";
      return filter === "live" ? isLive : !isLive;
    }),
  );

  function say(message: string): void {
    toast = message;
    if (toastTimer !== undefined) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 2600);
  }

  async function refresh(): Promise<void> {
    const wasConnected = store.connected;
    store = await refreshAll(store);
    // Pending rows keep their optimistic status: a poll that was already on
    // the wire returns pre-click truth and must not wipe mid-action feedback.
    applyPending();
    // Hosts and terminals are one-shot reads, not part of the poll: re-read
    // them when the daemon appears. The desktop shell loads the page before
    // the sidecar answers, so without this the sidebar stays empty until the
    // user refreshes by hand.
    if (!wasConnected && store.connected) {
      store = await refreshHosts(store);
      store = await refreshTerminals(store);
    }
  }

  function setBusy(id: string, on: boolean): void {
    const next = new Set(busyIds);
    if (on) next.add(id);
    else next.delete(id);
    busyIds = next;
  }

  /**
   * Rows with a command in flight. A poll that was already on the wire when
   * the user clicked returns pre-click truth and would wipe the optimistic
   * row back to `running` mid-action — the exact "flashed then stuck" the
   * pending map exists to prevent. Entries expire (see `pendingTtlMs`), so a
   * hung command wedges the row for seconds, never forever.
   */
  interface PendingAction {
    status: TunnelStatus;
    until: number;
  }
  let pending = $state(new Map<string, PendingAction>());

  /** Dead-man switch for one row action. Overridable with `?pendingTtlMs=` in tests. */
  function pendingTtlMs(): number {
    try {
      const raw = new URLSearchParams(window.location.search).get("pendingTtlMs");
      const parsed = raw === null ? NaN : Number(raw);
      if (Number.isFinite(parsed) && parsed > 0) return parsed;
    } catch {
      // Non-browser (prerender): fall through to the default.
    }
    return 45_000;
  }

  function patchRowStatus(id: string, status: TunnelStatus): void {
    store = {
      ...store,
      states: store.states.map((s) =>
        s.profileId === id ? { ...s, status } : s,
      ),
    };
  }

  function setPending(id: string, status: TunnelStatus | null): void {
    const next = new Map(pending);
    if (status === null) {
      next.delete(id);
    } else {
      next.set(id, { status, until: Date.now() + pendingTtlMs() });
      patchRowStatus(id, status);
    }
    pending = next;
  }

  /** Re-assert pending rows after every poll; drop expired entries first. */
  function applyPending(): void {
    if (pending.size === 0) return;
    const now = Date.now();
    let next: Map<string, PendingAction> | null = null;
    for (const [id, action] of pending) {
      if (action.until <= now) {
        if (next === null) next = new Map(pending);
        next.delete(id);
      }
    }
    if (next !== null) pending = next;
    if (pending.size === 0) return;
    store = {
      ...store,
      states: store.states.map((s) => {
        const action = pending.get(s.profileId);
        return action === undefined ? s : { ...s, status: action.status };
      }),
    };
  }

  /**
   * Run one row action with immediate feedback, in this order: loading state
   * first, command second. The UI never waits on the command to paint, and a
   * watchdog releases the row if the command never settles — the 3s poll
   * keeps watching the truth either way, so a timeout degrades to "still
   * watching", never to a wedged button.
   */
  async function act(
    id: string,
    label: string,
    fn: () => Promise<unknown>,
    optimistic?: TunnelStatus,
  ): Promise<void> {
    if (busyIds.has(id)) return;
    setBusy(id, true);
    if (optimistic !== undefined) setPending(id, optimistic);
    const watchdog = setTimeout(() => {
      // A successful poll clears the banner (`refreshAll` resets `lastError`
      // on fresh data), so a warning routed through it would be erased by the
      // very refresh below. The toast channel survives polls instead.
      setPending(id, null);
      setBusy(id, false);
      say(`${label} is taking longer than usual — still watching its status`);
      void refresh();
    }, pendingTtlMs());
    try {
      await fn();
      say(label);
    } catch (e) {
      store = { ...store, lastError: e instanceof Error ? e.message : "action failed" };
    } finally {
      clearTimeout(watchdog);
      setBusy(id, false);
      setPending(id, null);
      await refresh();
    }
  }

  function openCreate(): void {
    editing = null;
    editorError = null;
    editorOpen = true;
  }

  function openEdit(profile: TunnelProfile): void {
    editing = profile;
    editorError = null;
    editorOpen = true;
  }

  async function saveProfile(input: TunnelProfileInput): Promise<void> {
    saving = true;
    editorError = null;
    try {
      if (editing !== null) {
        await getClient().updateProfile(editing.id, input);
        say(`saved ${input.name}`);
      } else {
        await getClient().createProfile(input);
        say(`created ${input.name} — press Start to apply`);
      }
      editorOpen = false;
    } catch (e) {
      editorError = e instanceof Error ? e.message : "save failed";
    } finally {
      saving = false;
      await refresh();
    }
  }

  async function deleteProfile(id: string, name: string): Promise<void> {
    if (!confirm(`Delete tunnel "${name}"?`)) return;
    await act(id, `deleted ${name}`, () => getClient().deleteProfile(id));
  }

  async function copyCommand(id: string): Promise<void> {
    try {
      const { command } = await getClient().preview(id);
      await navigator.clipboard.writeText(command);
      say("command copied — paste it anywhere");
    } catch (e) {
      store = { ...store, lastError: e instanceof Error ? e.message : "copy failed" };
    }
  }

  function openLog(id: string): void {
    const profile = store.profiles.find((candidate) => candidate.id === id);
    if (profile === undefined) return;
    logProfile = profile;
    logOpen = true;
  }

  async function launch(id: string, terminalId: string, mode: LaunchMode): Promise<void> {
    setBusy(id, true);
    try {
      const session = await getClient().launchInTerminal(id, terminalId, mode);
      say(
        mode === "shell"
          ? `opened a shell in ${terminalId} (pid ${session.pid})`
          : `opened in ${terminalId} (pid ${session.pid})`,
      );
    } catch (e) {
      store = { ...store, lastError: e instanceof Error ? e.message : "launch failed" };
    } finally {
      setBusy(id, false);
      await refresh();
    }
  }

  async function stopAll(): Promise<void> {
    try {
      await getClient().stopAll();
      say("stopped all tunnels");
    } catch (e) {
      store = { ...store, lastError: e instanceof Error ? e.message : "stop-all failed" };
    }
    await refresh();
  }

  function terminalLabel(terminalId: string, terminals: readonly TerminalTarget[]): string {
    return terminals.find((t) => t.id === terminalId)?.label ?? terminalId;
  }

  onMount(() => {
    void (async () => {
      store = await refreshAll(store);
      store = await refreshHosts(store);
      store = await refreshTerminals(store);
    })();
    timer = setInterval(() => void refresh(), 3000);
    // Test hook: `?editor=open` auto-opens the creation dialog for
    // screenshots and e2e without shipping any test-only UI.
    try {
      const hook = new URLSearchParams(window.location.search);
      if (hook.get("editor") === "open") {
        editorOpen = true;
      }
      if (hook.get("settings") === "open") {
        settingsOpen = true;
      }
    } catch {
      // Non-browser (prerender): ignore.
    }
  });
  onDestroy(() => {
    if (timer !== undefined) clearInterval(timer);
    if (toastTimer !== undefined) clearTimeout(toastTimer);
  });
</script>

<div class="tunnel-shell">
  <!--
    Title bar. The native bar is hidden (Overlay + hiddenTitle), so this strip
    is the window's own title bar. `data-tauri-drag-region="deep"` makes every
    pixel of the strip draggable, including the text inside it — a bare
    attribute only matches clicks that land on that exact element, so the
    title's own glyphs would swallow the drag. The controls sit outside it and
    stay clickable.
  -->
  <header class="tunnel-toolbar">
    <div class="toolbar-title" data-tauri-drag-region="deep">
      <strong>TunnelDeck</strong>
      <span>one click to apply, one click to stop</span>
    </div>
    <span class="toolbar-spacer" data-tauri-drag-region="deep"></span>
    <div class="toolbar-actions glass-controls">
      <button class="toolbar-button primary" onclick={openCreate} title="New tunnel">
        <Plus size={14} /> New tunnel
      </button>
      <button class="toolbar-icon" onclick={() => void stopAll()} title="Stop all tunnels" aria-label="Stop all tunnels">
        <CircleStop size={14} />
      </button>
      <button class="toolbar-icon" onclick={() => void refresh()} title="Refresh" aria-label="Refresh">
        <RefreshCw size={14} />
      </button>
      <button class="toolbar-icon" onclick={() => (settingsOpen = true)} title="Settings" aria-label="Settings">
        <SettingsIcon size={14} />
      </button>
    </div>
  </header>

  <div class="tunnel-body">
    <!-- Sidebar -->
    <aside class="tunnel-sidebar">
      <div class="sidebar-scroll">
        <p class="sidebar-heading">Tunnels</p>
        <button class="sidebar-item" class:selected={filter === "all"} onclick={() => (filter = "all")}>
          <SquareTerminal size={15} /> <span>All</span>
          <span class="sidebar-count">{store.profiles.length}</span>
        </button>
        <button class="sidebar-item" class:selected={filter === "live"} onclick={() => (filter = "live")}>
          <Wifi size={15} /> <span>Running</span>
          <span class="sidebar-count">{running}</span>
        </button>
        <button class="sidebar-item" class:selected={filter === "idle"} onclick={() => (filter = "idle")}>
          <WifiOff size={15} /> <span>Stopped</span>
          <span class="sidebar-count">{store.profiles.length - running}</span>
        </button>

        {#if store.hosts.length}
          <p class="sidebar-heading">SSH hosts</p>
          {#each store.hosts.slice(0, 12) as host (host.alias)}
            <div class="sidebar-item" title={host.alias}>
              <SquareTerminal size={15} /> <span>{host.alias}</span>
            </div>
          {/each}
        {/if}

        {#if store.terminals.length}
          <p class="sidebar-heading">Terminals</p>
          {#each store.terminals as terminal (terminal.id)}
            <div class="sidebar-item" title={terminal.hint}>
              <SquareTerminal size={15} /> <span>{terminal.label}</span>
            </div>
          {/each}
        {/if}
      </div>
    </aside>

    <!-- Content -->
    <main class="tunnel-content">
      <div class="content-heading">
        <h2>Tunnels</h2>
        <p>{visible.length} of {store.profiles.length} shown</p>
      </div>

      {#if store.lastError}
        <p class="mb-3 rounded-[var(--radius)] border border-destructive/40 bg-destructive/10 px-3 py-2 text-[12px] text-destructive">
          {store.lastError}
        </p>
      {/if}

      {#if store.profiles.length === 0 && !store.loading}
        <div class="content-empty">
          <SquareTerminal size={30} />
          <p class="text-[13px] text-foreground">No tunnels yet</p>
          <p class="text-[12px]">Create one — your SSH aliases are suggested in the target field.</p>
          <button class="toolbar-button primary mt-2" onclick={openCreate}><Plus size={14} /> New tunnel</button>
        </div>
      {:else if visible.length === 0}
        <div class="content-empty">
          <p class="text-[12px]">Nothing matches this filter.</p>
        </div>
      {:else}
        <div class="tunnel-table">
          {#each visible as profile (profile.id)}
            <TunnelRow
              {profile}
              tunnelState={store.states.find((s) => s.profileId === profile.id)}
              busy={busyIds.has(profile.id)}
              terminals={store.terminals}
              onStart={(id) => act(id, "started", () => getClient().start(id), "starting")}
              onStop={(id) => act(id, "stopped", () => getClient().stop(id), "stopping")}
              onProbe={(id) => act(id, "probed", () => getClient().probe(id))}
              onCopy={(id) => void copyCommand(id)}
              onLaunch={launch}
              onLog={openLog}
              onEdit={openEdit}
              onDelete={deleteProfile}
            />
          {/each}
        </div>
      {/if}
    </main>
  </div>

  <!-- Status bar -->
  <footer class="tunnel-status">
    <span>
      <strong>{running}</strong> running
      <span class="separator">·</span>
      <strong>{store.profiles.length}</strong> tunnels
      {#if failed > 0}
        <span class="separator">·</span>
        <strong class="text-destructive">{failed}</strong> failed
      {/if}
      <span class="separator">·</span>
      <strong>{store.hosts.length}</strong> ssh hosts
    </span>
    <span>
      <i class:offline={!store.connected}></i>
      {store.connected ? "daemon connected" : store.loading ? "connecting…" : "daemon offline"}
    </span>
  </footer>
</div>

<SettingsSheet
  open={settingsOpen}
  {hasMaterial}
  opacity={materialOpacity}
  onOpacity={applyOpacity}
  onClose={() => (settingsOpen = false)}
/>

<LogSheet open={logOpen} profile={logProfile} onClose={() => (logOpen = false)} />

<TunnelEditor
  open={editorOpen}
  {editing}
  hosts={store.hosts}
  {saving}
  error={editorError}
  onSave={(input) => void saveProfile(input)}
  onClose={() => (editorOpen = false)}
/>

{#if toast}
  <div class="fixed bottom-10 left-1/2 z-50 -translate-x-1/2 rounded-[var(--radius)] border border-border bg-popover px-3 py-1.5 text-[12px] text-popover-foreground shadow-[0_8px_24px_rgb(0_0_0/0.2)]">
    {toast}
  </div>
{/if}