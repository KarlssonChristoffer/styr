<!-- A command's output as its terminal showed it: the last `max` lines, colours kept. With `scroll` it is a box of its own that follows new lines unless you scrolled up to read.
     Used by ThreadPanel.svelte, Transcript.svelte. -->
<script lang="ts">
  // A command's output as its terminal showed it: the last `max` lines, colours kept. With
  // `scroll` it is a box of its own that follows new lines unless you scrolled up to read.
  import { screen } from "../ansi";

  let {
    text,
    max = 400,
    scroll = false,
    cut = false,
    earlier = true,
  }: {
    text: string;
    max?: number;
    scroll?: boolean;
    /** The start was dropped already. */
    cut?: boolean;
    /** Say how many lines are above the ones shown. */
    earlier?: boolean;
  } = $props();

  const shown = $derived(screen(text, max));
  let box = $state<HTMLPreElement>();
  let stick = true;

  $effect.pre(() => {
    void shown;
    if (box) stick = box.scrollHeight - box.scrollTop - box.clientHeight < 24;
  });
  $effect(() => {
    void shown;
    if (scroll && stick && box) box.scrollTop = box.scrollHeight;
  });
</script>

<pre class="out" class:scroll bind:this={box}>{#if earlier && (shown.earlier || cut)}<span class="earlier">{cut ? "Earlier output not kept" : `${shown.earlier} earlier ${shown.earlier === 1 ? "line" : "lines"}`}
</span>{/if}{#each shown.lines as line, i (i)}{#each line as span, j (j)}{#if span.cls}<span class={span.cls}>{span.text}</span>{:else}{span.text}{/if}{/each}{#if i < shown.lines.length - 1}{"\n"}{/if}{/each}</pre>

<style>
  .out {
    user-select: text; -webkit-user-select: text;
    margin: 0; font-family: var(--mono); font-size: 12px; line-height: 1.5; color: var(--text-3);
    white-space: pre-wrap; word-break: break-word; tab-size: 4;
  }
  .out.scroll { max-height: 280px; overflow: auto; }
  .earlier { color: var(--muted); font-style: italic; }
  .b { font-weight: 600; color: var(--text-2); }
  .d { opacity: 0.7; }
  /* Output colours are the theme's state colours, a little quieter than in the UI itself. */
  .c-red { color: var(--red); }
  .c-green { color: var(--green); }
  .c-amber { color: var(--amber); }
  .c-blue, .c-cyan, .c-magenta { color: var(--accent); }
  .c-dim { color: var(--muted); }
</style>
