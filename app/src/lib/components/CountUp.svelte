<!-- A percentage that counts to its new value instead of jumping.
     Used by UserMenu.svelte. -->
<script lang="ts">
  import { untrack } from "svelte";
  // A percentage that counts to its new value instead of jumping.
  import { Tween } from "svelte/motion";
  import { cubicOut } from "svelte/easing";
  import { SWITCH_MS, still } from "../cardSwitch";

  let { value }: { value: number } = $props();
  const shown = new Tween(untrack(() => value), { duration: SWITCH_MS, easing: cubicOut });
  $effect(() => {
    shown.set(value, still() ? { duration: 0 } : undefined);
  });
</script>

{Math.round(shown.current)}%
