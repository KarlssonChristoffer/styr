<!-- A source file to read: coloured by its language, with line numbers, its language and size in a bar, Copy, and a switch between scrolling long lines and wrapping them.
     Used by FilePreview.svelte, MachineView.svelte. -->
<script lang="ts">
  import { highlight, labelOf, languageOf } from "../code";
  import { copy } from "../menu.svelte";
  import { hub } from "../state.svelte";

  // A source file to read: coloured by its language, with line numbers, its language and size in
  // a bar, Copy, and a switch between scrolling long lines and wrapping them.
  let { path, text }: { path: string; text: string } = $props();

  const lang = $derived(languageOf(path));
  // Very big files stay plain; colouring them would hold the window up.
  const shown = $derived(text.length < 400_000 ? highlight(text, lang) : { html: "", lang });
  const lines = $derived(text.split("\n").length - (text.endsWith("\n") ? 1 : 0));
  let wrap = $state(false);
  let copied = $state(false);
  async function copyAll() {
    try {
      await copy(text);
    } catch (e) {
      return hub.fail(e);
    }
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<div class="code-view">
  <div class="bar">
    <span class="lang">{labelOf(shown.lang)}</span>
    <span>{lines} {lines === 1 ? "line" : "lines"}</span>
    <button class="tool" class:on={wrap} onclick={() => (wrap = !wrap)} data-tip="Wrap long lines">Wrap</button>
    <button class="tool" onclick={copyAll}>{copied ? "Copied" : "Copy"}</button>
  </div>
  <div class="body" class:wrap>
    <div class="nums mono" aria-hidden="true">{#each { length: lines } as _, i (i)}<span>{i + 1}</span>{/each}</div>
    {#if shown.html}
      <pre><code class="hljs">{@html shown.html}</code></pre>
    {:else}
      <pre><code>{text}</code></pre>
    {/if}
  </div>
</div>

<style>
  .code-view { background: var(--bg-sidebar); border: 1px solid var(--line); border-radius: var(--r-md); overflow: hidden; }
  .bar { display: flex; align-items: center; gap: 12px; padding: 4px 8px 4px 12px; border-bottom: 1px solid var(--line); color: var(--muted); font-size: 12px; }
  .lang { color: var(--text-3); }
  .tool { color: var(--muted); padding: 1px 8px; border-radius: var(--r-sm); font-size: 12px; }
  .tool:first-of-type { margin-left: auto; }
  .tool:hover, .tool.on { color: var(--text); background: var(--surface-hover); }
  .body { user-select: text; -webkit-user-select: text; display: flex; overflow-x: auto; font: 13px/1.6 var(--mono); }
  .nums { flex: none; display: flex; flex-direction: column; text-align: right; padding: 10px 10px 10px 12px; color: var(--muted); opacity: 0.6; user-select: none; border-right: 1px solid var(--line); position: sticky; left: 0; background: var(--bg-sidebar); }
  pre { margin: 0; padding: 10px 14px; flex: 1; min-width: 0; color: var(--text-2); }
  .wrap pre { white-space: pre-wrap; overflow-wrap: anywhere; }
  .wrap .nums { display: none; }
</style>
