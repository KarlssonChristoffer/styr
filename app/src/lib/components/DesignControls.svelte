<!-- The Design switch and, in design mode, the button that has fresh readers review the design
     document. Used by Composer.svelte. -->
<script lang="ts">
  import Icon from "./Icon.svelte";

  let {
    design,
    ondesign,
    ondesignreview,
    onerror,
  }: {
    design: boolean;
    /** Turns design mode on or off. */
    ondesign: (on: boolean) => void;
    ondesignreview?: () => Promise<void> | void;
    onerror?: (e: unknown) => void;
  } = $props();

  let reviewingDesign = $state(false);
</script>

<button
  class="pill"
  class:on={design}
  aria-pressed={design}
  data-tip={design
    ? "Design mode: the agent plans in a design document under docs/design/ and writes no code. Click to let it build"
    : "Design first: for something big, the agent plans in a design document and fresh reviewers check it before any code"}
  onclick={() => ondesign?.(!design)}
>
  <Icon name="design" size={14} /><span class="label">Design</span>
</button>
{#if design && ondesignreview}
  <button
    class="pill"
    disabled={reviewingDesign}
    data-tip="Fresh readers who never saw this conversation read the design document: one explains it back, one looks for gaps, one checks it against the code, one says if it is ready to build"
    onclick={async () => {
      reviewingDesign = true;
      try {
        await ondesignreview?.();
      } catch (e) {
        onerror?.(e);
      } finally {
        reviewingDesign = false;
      }
    }}
  >
    <Icon name="eye" size={14} /><span class="label">Review design</span>
  </button>
{/if}

<style>
  /* @kit menus */
</style>
