<!-- A log to look through: each line coloured by its level (errors red, warnings amber), terminal colours kept, buttons to show only errors or warnings, a filter, and the end shown first.
     Used by FilePreview.svelte. -->
<script lang="ts">
  // A log to look through: each line coloured by its level (errors red, warnings amber), terminal
  // colours kept, buttons to show only errors or warnings, a filter, and the end shown first.
  import { parse } from "../ansi";
  import { logLevel, type LogLevel } from "../filePreview";

  let { text }: { text: string } = $props();

  const lines = $derived(parse(text.replace(/\n$/, "")).map((spans, i) => {
    const plain = spans.map((s) => s.text).join("");
    return { i, spans, plain, level: logLevel(plain) };
  }));
  // A line without a level (a stack frame, a wrapped message) belongs to the line above it.
  const levels = $derived.by(() => {
    let last: LogLevel = null;
    return lines.map((l) => (l.level ? (last = l.level) : /^\s/.test(l.plain) ? last : ((last = null), null)));
  });
  // Counted by the lines that say their level, so a stack trace is one error, not ten.
  const counts = $derived({ error: lines.filter((l) => l.level === "error" && !/^\s/.test(l.plain)).length, warn: lines.filter((l) => l.level === "warn" && !/^\s/.test(l.plain)).length });
  let only = $state<"error" | "warn" | null>(null);
  let filter = $state("");
  const shown = $derived(
    lines.filter((l) => {
      const lv = levels[l.i];
      if (only === "error" && lv !== "error") return false;
      if (only === "warn" && lv !== "error" && lv !== "warn") return false;
      return !filter || l.plain.toLowerCase().includes(filter.toLowerCase());
    }),
  );
  const MAX = 5000;

  let box = $state<HTMLElement>();
  $effect(() => {
    void shown;
    if (box) box.scrollTop = box.scrollHeight;
  });
</script>

<div class="log">
  <div class="bar">
    <span>{lines.length} lines</span>
    <button class="err" class:on={only === "error"} disabled={!counts.error} onclick={() => (only = only === "error" ? null : "error")}>{counts.error} errors</button>
    <button class="warn" class:on={only === "warn"} disabled={!counts.warn} onclick={() => (only = only === "warn" ? null : "warn")}>{counts.warn} warnings</button>
    <input class="filter" placeholder="Filter lines" bind:value={filter} />
  </div>
  <pre bind:this={box}>{#if shown.length > MAX}<span class="d">{shown.length - MAX} earlier lines not shown
</span>{/if}{#each shown.slice(-MAX) as l (l.i)}<span class="line {levels[l.i] ?? ''}"><span class="no">{l.i + 1}</span>{#each l.spans as s, j (j)}{#if s.cls}<span class={s.cls}>{s.text}</span>{:else}{s.text}{/if}{/each}
</span>{/each}</pre>
</div>

<style>
  .log { user-select: text; -webkit-user-select: text; display: flex; flex-direction: column; min-height: 0; }
  .bar { user-select: none; -webkit-user-select: none; display: flex; align-items: center; gap: 8px; padding: 0 0 6px; color: var(--muted); font-size: 12px; }
  .bar button { padding: 1px 8px; border-radius: var(--r-sm); font-size: 12px; color: var(--muted); }
  .bar button:disabled { opacity: 0.4; }
  .bar button.err:not(:disabled) { color: var(--red); }
  .bar button.warn:not(:disabled) { color: var(--amber); }
  .bar button.on { background: var(--surface-hover); }
  .filter { margin-left: auto; width: 160px; height: 22px; padding: 0 8px; border-radius: var(--r-sm); border: 1px solid var(--line); background: var(--surface); color: var(--text-2); font-size: 12px; }
  .filter:focus { outline: none; border-color: var(--line-strong); }
  pre { margin: 0; padding: 8px 0; overflow: auto; max-height: inherit; flex: 1; min-height: 0; background: var(--bg-sidebar); border: 1px solid var(--line); border-radius: var(--r-md); font: 12px/1.55 var(--mono); color: var(--text-2); }
  .line { display: block; padding: 0 12px 0 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .line.error { color: var(--red); background: var(--red-bg); }
  .line.warn { color: var(--amber); }
  .line.debug { color: var(--muted); }
  .no { display: inline-block; width: 4.5em; padding-right: 10px; text-align: right; color: var(--muted); opacity: 0.6; user-select: none; }
  .d, :global(.log .d) { color: var(--muted); }
  .log :global(.c-red) { color: var(--red); }
  .log :global(.c-green) { color: var(--green); }
  .log :global(.c-amber) { color: var(--amber); }
  .log :global(.c-blue), .log :global(.c-cyan), .log :global(.c-magenta) { color: var(--accent); }
  .log :global(.c-dim) { color: var(--muted); }
  .log :global(.b) { font-weight: 600; }
</style>
