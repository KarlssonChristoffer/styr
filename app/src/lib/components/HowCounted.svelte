<!-- How a card's numbers are counted. A card writes it once, here, and the board shows it when the card's right-click menu asks (How it's counted): nothing is drawn in the card itself.
     Used by ActivityMap.svelte, ClockCard.svelte, DaylightCard.svelte, HealthCard.svelte and 12 more. -->
<script lang="ts">
  // How a card's numbers are counted. A card writes it once, here, and the board shows it when
  // the card's right-click menu asks (How it's counted): nothing is drawn in the card itself.
  import type { Snippet } from "svelte";
  import { about } from "../overview.svelte";

  let { card, children }: { card: string; children: Snippet } = $props();
  $effect(() => {
    about[card] = children;
    return () => {
      delete about[card];
    };
  });
</script>
