<!-- A model provider's logo by its catalog id (`engines.rs`): DeepSeek, OpenRouter, Kimi, Z.ai, xAI. Unknown ones get their first letter in a rounded square, in `color` when one is given. API connections use it too, by their kind.
     Used by ApiConnections.svelte, FolderPicker.svelte, JevBar.svelte, PlusMenu.svelte and 3 more. -->
<script lang="ts">
  // A model provider's logo by its catalog id (`engines.rs`): DeepSeek, OpenRouter, Kimi, Z.ai,
  // xAI. Unknown ones get their first letter in a rounded square,
  // in `color` when one is given. API connections use it too, by their kind.
  import { LOGOS } from "../providerLogos";

  let { id, label = "", size = 20, color = "" }: { id: string; label?: string; size?: number; color?: string } = $props();
  const logo = $derived(LOGOS[id]);
</script>

{#if logo}
  <svg class="logo" width={size} height={size} viewBox="0 0 {logo.box ?? 24} {logo.box ?? 24}" aria-hidden="true" style:color={logo.color ?? "var(--text)"}>
    <path fill="currentColor" d={logo.path} />
  </svg>
{:else}
  <span class="letter" style:width="{size}px" style:height="{size}px" style:font-size="{Math.round(size * 0.55)}px"
    style:color={color || null} style:background={color ? `color-mix(in srgb, ${color} 18%, transparent)` : null}>{(label || id).slice(0, 1).toUpperCase()}</span>
{/if}

<style>
  .logo { flex: none; display: block; }
  .letter { flex: none; display: inline-flex; align-items: center; justify-content: center; border-radius: var(--r-sm); background: var(--surface-hover); color: var(--text-2); font-weight: 600; }
</style>
