<!--
  Settings: appearance preferences the daemon has no opinion about.

  Only the material opacity lives here today. It is a page preference, not
  tunnel state, so it is stored in localStorage and applied straight to the
  document's custom properties — the desktop shell stays out of the loop and
  the value survives a reload without a round trip.
-->
<script lang="ts">
  import { X } from "@lucide/svelte";
  import UpdateSection from "$lib/UpdateSection.svelte";

  interface Props {
    open: boolean;
    /** True only when the desktop host really installed a native material. */
    hasMaterial: boolean;
    opacity: number;
    onOpacity: (value: number) => void;
    onClose: () => void;
  }

  import { runsInTauri } from "$lib/store";

  let { open, hasMaterial, opacity, onOpacity, onClose }: Props = $props();

  // macOS labels this "Reduce transparency"; keep the sentence familiar.
  const labels: readonly { at: number; text: string }[] = [
    { at: 0.5, text: "mostly opaque" },
    { at: 0.72, text: "balanced" },
    { at: 0.9, text: "mostly transparent" },
  ];

  const hint = $derived(
    labels.reduce((best, entry) =>
      Math.abs(entry.at - opacity) < Math.abs(best.at - opacity) ? entry : best,
    ).text,
  );
</script>

{#if open}
  <div class="sheet-scrim fixed inset-0 z-50 flex items-center justify-center p-4" role="presentation" tabindex="-1" onclick={onClose} onkeydown={(e) => e.key === "Escape" && onClose()}>
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
      tabindex="-1"
      class="tunnel-dialog w-[420px] p-5"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.key === "Escape" && onClose()}
    >
      <div class="mb-4 flex items-center justify-between">
        <h2 class="text-[13px] font-semibold">Settings</h2>
        <button class="toolbar-icon" onclick={onClose} title="Close" aria-label="Close">
          <X size={14} />
        </button>
      </div>

      <div class="flex flex-col gap-2">
        <div class="flex items-baseline justify-between">
          <label class="text-[12px]" for="material-opacity">Window transparency</label>
          <span class="text-[11px] text-muted-foreground">{hint}</span>
        </div>
        <input
          id="material-opacity"
          type="range"
          min="0.4"
          max="0.94"
          step="0.02"
          value={opacity}
          disabled={!hasMaterial}
          oninput={(event) => onOpacity(Number(event.currentTarget.value))}
        />
        <p class="text-[10px] leading-snug text-muted-foreground">
          {#if hasMaterial}
            How much of your desktop shows through the window. Lower values keep text easier to read; the sidebar, title bar and content always move together.
          {:else}
            This window has no native material behind it — a browser tab cannot be translucent — so the control is inactive. It takes effect in the desktop app on macOS and Windows.
          {/if}
        </p>
      </div>

      {#if runsInTauri()}
        <UpdateSection />
      {/if}
    </div>
  </div>
{/if}