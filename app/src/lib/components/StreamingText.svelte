<!-- Reveals a streaming reply at an even pace, so text that arrives in large chunks still reads as written live. It catches up faster the further behind it is, and shows everything at once when the reply is no longer streaming.
     Used by Transcript.svelte. -->
<script lang="ts">
  // Reveals a streaming reply at an even pace, so text that arrives in large chunks still reads
  // as written live. It catches up faster the further behind it is, and shows everything at once
  // when the reply is no longer streaming.
  import { onDestroy, untrack } from "svelte";
  import Markdown from "./Markdown.svelte";

  let { text, streaming, rich = false }: { text: string; streaming: boolean; rich?: boolean } = $props();

  let shown = $state(untrack(() => (streaming ? 0 : text.length)));
  let frame = 0;

  function tick() {
    const behind = text.length - shown;
    if (behind <= 0) {
      frame = 0;
      return;
    }
    shown = Math.min(text.length, shown + Math.max(2, Math.ceil(behind / 12)));
    frame = requestAnimationFrame(tick);
  }

  $effect(() => {
    void text;
    if (!streaming) {
      shown = text.length;
      return;
    }
    if (!frame && shown < text.length) frame = requestAnimationFrame(tick);
  });
  onDestroy(() => cancelAnimationFrame(frame));
</script>

<Markdown {rich} text={shown >= text.length ? text : text.slice(0, shown)} />
