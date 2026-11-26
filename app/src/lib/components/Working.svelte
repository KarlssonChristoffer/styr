<!-- "Something is going on here", the one way Hub says it: the working grid (`WorkGrid.svelte`) in the loop chosen in Settings (Pour by default), at the size of the text beside it. It takes the place of every turning ring. `seed` keeps a Random pick the same for one thing.
     Used by ActivityList.svelte, ChildLine.svelte, EngineList.svelte, OverviewShelf.svelte and 4 more. -->
<script lang="ts">
  // "Something is going on here", the one way Hub says it: the working grid (`WorkGrid.svelte`)
  // in the loop chosen in Settings (Pour by default), at the size of the text beside it. It takes
  // the place of every turning ring. `seed` keeps a Random pick the same for one thing.
  import { prefs } from "../prefs.svelte";
  import { workShape } from "../workGrid";
  import WorkGrid from "./WorkGrid.svelte";

  let { size = 12, seed = "" }: { size?: number; seed?: string } = $props();
</script>

<span class="working" aria-hidden="true"><WorkGrid shape={workShape(seed, prefs.workIndicator)} {size} /></span>

<style>
  .working { display: inline-flex; flex: none; vertical-align: middle; }
</style>
