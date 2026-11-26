<!-- Ctrl F in a thread: a small box over the top of the conversation. Enter goes to the next match, Shift Enter back, Escape closes.
     Used by ThreadView.svelte. -->
<script lang="ts">
  import { slide } from "../pop";
  // Ctrl F in a thread: a small box over the top of the conversation. Enter goes to the next
  // match, Shift Enter back, Escape closes.
  import Icon from "./Icon.svelte";

  let {
    query = $bindable(""),
    count,
    at,
    onnext,
    onprev,
    onclose,
  }: { query: string; count: number; at: number; onnext: () => void; onprev: () => void; onclose: () => void } = $props();

  let input: HTMLInputElement;

  export function focus() {
    input?.focus();
    input?.select();
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === "F3") {
      e.preventDefault();
      if (e.shiftKey) onprev();
      else onnext();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    }
  }
</script>

<div class="find" in:slide|global out:slide|global={{ out: true }} role="search" data-find-skip>
  <Icon name="search" size={13} />
  <input bind:this={input} bind:value={query} placeholder="Find in this thread" onkeydown={key} spellcheck="false" aria-label="Find in this thread" />
  <span class="count mono" class:none={!!query.trim() && !count}>{query.trim() ? (count ? `${at + 1}/${count}` : "0/0") : ""}</span>
  <button class="ghost" data-tip="Previous (Shift Enter)" disabled={!count} onclick={onprev}><span class="flip"><Icon name="down" size={13} /></span></button>
  <button class="ghost" data-tip="Next (Enter)" disabled={!count} onclick={onnext}><Icon name="down" size={13} /></button>
  <button class="ghost" data-tip="Close (Esc)" onclick={onclose}><Icon name="x" size={13} /></button>
</div>

<style>
  .find {
    position: absolute; top: 8px; right: 20px; z-index: 6; display: flex; align-items: center; gap: 4px;
    padding: 4px 4px 4px 10px; color: var(--muted); background: var(--surface-2);
    border: 1px solid var(--line-strong); border-radius: var(--r-md); box-shadow: 0 8px 24px rgba(0, 0, 0, 0.28);
  }
  input { width: 200px; background: none; border: 0; outline: none; color: var(--text); font-size: 13px; padding: 3px 4px; }
  .count { min-width: 42px; text-align: right; font-size: 12px; color: var(--muted); }
  .count.none { color: var(--red); }
  .mono { font-family: var(--mono); }
  .ghost { color: var(--muted); display: grid; place-items: center; padding: 4px; border-radius: var(--r-sm); }
  .ghost:hover:not(:disabled) { color: var(--text); background: var(--surface-hover); }
  .ghost:disabled { opacity: 0.4; }
  .flip { display: grid; transform: rotate(180deg); }
</style>
