<!-- Which machine something runs on: icon and name, never colour (colour is for status). Grey while the machine is offline.
     Used by FolderPicker.svelte, Home.svelte, ProjectView.svelte, Settings.svelte and 2 more. -->
<script lang="ts">
  import { hub } from "../state.svelte";
  import { GLYPHS, machineGlyph } from "../refs";

  // Which machine something runs on: icon and name, never colour (colour is for status). Grey
  // while the machine is offline.
  let { name, compact = false }: { name: string; compact?: boolean } = $props();

  const info = $derived(hub.hostInfo(name));
  const icon = $derived(machineGlyph(name, !!info?.local));
  const offline = $derived(!!info && info.state !== "online");
  const title = $derived(
    !info ? name
    : info.state === "offline" ? `${name} is offline${info.error ? `: ${info.error}` : ""}`
    : info.state === "connecting" ? `Connecting to ${name}…`
    : info.local ? `${name}, this machine` : `${name} · ${info.address}`,
  );
</script>

<span class="chip" class:offline class:compact data-tip={title}>
  <svg class="kind" width={compact ? 12 : 13} height={compact ? 12 : 13} viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS[icon]} /></svg>{#if !compact}<span>{name}</span>{/if}
</span>

<style>
  .chip { display: inline-flex; align-items: center; gap: 4px; color: var(--text-3); white-space: nowrap; flex: none; }
  .chip.compact { color: var(--muted); }
  .kind { flex: none; fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
  .chip.offline { color: var(--muted); opacity: 0.6; }
</style>
