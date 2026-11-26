<!-- One icon that changes shape (copy to check, play to pause): the old one leaves while the new one arrives in the same spot, scale .8 to 1, opacity and blur 2px to 0, so no empty frame and no pop between two shapes (design/ANIMATION.md rule 16 and 18). Reduced motion (system or Settings) swaps at once.
     Used by Changes.svelte, Composer.svelte, DesignCard.svelte, FolderPicker.svelte and 6 more. -->
<script lang="ts">
  // One icon that changes shape (copy to check, play to pause): the old one leaves while the new one
  // arrives in the same spot, scale .8 to 1, opacity and blur 2px to 0, so no empty frame and no pop
  // between two shapes (design/ANIMATION.md rule 16 and 18). Reduced motion (system or Settings) swaps at once.
  import { cubicOut } from "svelte/easing";
  import { still } from "../pop";
  import Icon, { type IconName } from "./Icon.svelte";

  let { name, size = 16 }: { name: IconName; size?: number } = $props();

  const fade = (_: Element, o: { duration: number }) => ({
    duration: still() ? 0 : o.duration,
    easing: cubicOut,
    css: (t: number, u: number) => `opacity:${t};transform:scale(${0.8 + 0.2 * t});filter:blur(${2 * u}px)`,
  });
</script>

<span class="swap" style:width="{size}px" style:height="{size}px">
  {#key name}
    <span class="one" in:fade={{ duration: 160 }} out:fade={{ duration: 130 }}><Icon {name} {size} /></span>
  {/key}
</span>

<style>
  .swap { display: inline-grid; flex: none; vertical-align: middle; }
  .one { grid-area: 1 / 1; display: flex; align-items: center; justify-content: center; }
</style>
