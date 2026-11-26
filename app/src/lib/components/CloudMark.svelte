<!-- Where a cloud thread runs: the cloud icon alone, in the place of the machine's. The engine's own mark stands next to it, so the two together say "Codex, in the cloud".
     Used by ActivityList.svelte, ThreadView.svelte. -->
<script lang="ts">
  // Where a cloud thread runs: the cloud icon alone, in the place of the machine's. The engine's
  // own mark stands next to it, so the two together say "Codex, in the cloud".
  import { cloudHost, cloudLabel, type CloudRun } from "../cloud.svelte";
  import { GLYPHS } from "../refs";

  let { cloud, size = 13 }: { cloud: CloudRun; size?: number } = $props();
</script>

<span class="cloud" data-tip="{cloudLabel(cloud.provider)}: runs on {cloudHost(cloud.provider)}, not on a machine of yours">
  <svg class="kind" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS.cloudrun} /></svg>
</span>

<style>
  .cloud { display: inline-flex; align-items: center; color: var(--muted); flex: none; }
  .kind { fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
</style>
