<!-- A break: one wavy line across the chat where something changed, in a quiet colour per kind (the way ChatGPT marks "New" after a pause). The colour sits on the line and the glyph only; the words stay grey and nothing is boxed. Every kind of break in a thread uses this, so a new kind is a new `kind`, never a new look. Colour says what sort of break it is, not a state.
     Used by Transcript.svelte. -->
<script lang="ts" module>
  /** engine: another engine took over. machine: the thread moved. session: a new session from a
   *  summary. waiting: held up until a reset. stuck: nothing can go on. time: a pause in between.
   *  new: what happened while you were away starts here. */
  export type BreakKind = "engine" | "machine" | "session" | "waiting" | "stuck" | "time" | "new";
</script>

<script lang="ts">
  // A break: one wavy line across the chat where something changed, in a quiet colour per kind
  // (the way ChatGPT marks "New" after a pause). The colour sits on the line and the glyph only;
  // the words stay grey and nothing is boxed. Every kind of break in a thread uses this, so a new
  // kind is a new `kind`, never a new look. Colour says what sort of break it is, not a state.
  import type { Snippet } from "svelte";

  let { kind, children, note, hint }: { kind: BreakKind; children: Snippet; note?: string; hint?: Snippet } = $props();
</script>

<div class="break {kind}" role="separator">
  <span class="wave"></span>
  <span class="words">{@render children()}{#if note}<span class="note">{note}</span>{/if}</span>
  <span class="wave"></span>
</div>
{#if hint}<div class="hint">{@render hint()}</div>{/if}

<style>
  .break { --c: var(--muted); display: flex; align-items: center; gap: 14px; margin: 22px 0; color: var(--text-3); font-size: 13px; }
  .engine { --c: var(--p-violet); }
  .machine { --c: var(--p-teal); }
  .session { --c: var(--p-sky); }
  .waiting { --c: var(--p-sand); }
  .stuck { --c: var(--p-rose); }
  .time { --c: var(--muted); }
  .new { --c: var(--p-orange); }
  .new .words { color: var(--p-orange); }
  .wave {
    flex: 1; min-width: 20px; height: 8px; background: var(--c); opacity: 0.7;
    -webkit-mask: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='24' height='8'><path d='M0 4 Q6 0 12 4 T24 4' fill='none' stroke='black' stroke-width='1.3'/></svg>") repeat-x center / 24px 8px;
    mask: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='24' height='8'><path d='M0 4 Q6 0 12 4 T24 4' fill='none' stroke='black' stroke-width='1.3'/></svg>") repeat-x center / 24px 8px;
  }
  .words { display: inline-flex; align-items: center; gap: 8px; white-space: nowrap; max-width: 80%; }
  .words :global(svg) { flex: none; }
  .words :global(svg.glyph) { stroke: var(--c); margin-right: 0; width: 14px; height: 14px; }
  .note { color: var(--muted); font-size: 12px; margin-left: 2px; overflow: hidden; text-overflow: ellipsis; }
  .hint { display: flex; justify-content: center; gap: 4px; flex-wrap: wrap; margin: -12px 0 20px; color: var(--muted); font-size: 13px; text-align: center; }
  .hint :global(button) { color: var(--text-3); text-decoration: underline; text-underline-offset: 2px; padding: 0 2px; font-size: inherit; }
  .hint :global(button:hover) { color: var(--text); }
</style>
