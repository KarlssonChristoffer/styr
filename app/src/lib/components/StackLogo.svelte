<!-- A language, framework or tool in its own colour, from the logo library (stackLogos.ts); a folder for no logo or one the library does not have.
     Used by MachineView.svelte, NewProject.svelte. -->
<script lang="ts">
  import { LOGOS, logoColor } from "../stackLogos";
  import Icon from "./Icon.svelte";

  // A language, framework or tool in its own colour, from the logo library (stackLogos.ts); a
  // folder for no logo or one the library does not have.
  let { id, size = 16 }: { id: string | null; size?: number } = $props();
  const logo = $derived(id ? LOGOS[id] : undefined);
</script>

{#if logo}
  <svg width={size} height={size} viewBox="0 0 24 24" fill={logoColor(logo.hex)} role="img" aria-label={logo.name}><path d={logo.path} /></svg>
{:else}
  <span class="none"><Icon name="folder" size={size} /></span>
{/if}

<style>
  svg, .none { flex: none; display: block; }
  .none { color: var(--muted); }
</style>
