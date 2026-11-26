<!-- Needs you: the threads that wait on you (a question, a failure, a reply that asks), what runs meanwhile, and what is left to test.
     Used by Scenes.svelte. -->
<script lang="ts">
  import Quiet from "../Quiet.svelte";
  // Needs you: the threads that wait on you (a question, a failure, a reply that asks), what runs
  // meanwhile, and what is left to test.
  import { ago } from "../../format";
  import { hub } from "../../state.svelte";
  import { waitsOnYou } from "../../state.svelte";
  import ThreadRows from "./ThreadRows.svelte";
  import ToTest from "./ToTest.svelte";

  const asks = $derived(hub.active.filter(waitsOnYou).sort((a, b) => (a.returned_at ?? a.updated_at) - (b.returned_at ?? b.updated_at)));
  const oldest = $derived(asks[0] ? ago(asks[0].returned_at ?? asks[0].updated_at) : "");
</script>

<section class="s12 c"><div class="lbl">Needs you <span class="n">{asks.length}</span>{#if oldest}<span class="r">oldest {oldest}</span>{/if}</div>
  <div class="blk"><ThreadRows threads={asks} empty="Nothing waits on you." open="Open" /></div></section>
<section class="s12 c"><div class="lbl">Running <span class="n">{hub.running.length}</span></div>
  <div class="blk"><ThreadRows threads={hub.running.slice(0, 5)} empty="Nothing is running." clock /></div></section>
<section class="s12"><div class="lbl">To test by hand</div><div class="blk"><ToTest /></div></section>

<style>
</style>
