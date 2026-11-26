<!-- Offers to turn a long, messy message (often dictated) into a clear prompt. The tidy version replaces the text in the box, so it can still be edited; the original goes along to the log.
     Used by Composer.svelte. -->
<script lang="ts">
  // Offers to turn a long, messy message (often dictated) into a clear prompt. The tidy version
  // replaces the text in the box, so it can still be edited; the original goes along to the log.
  import { api } from "../api";

  /** Shorter messages are left alone. */
  const LONG = 900;

  let {
    text,
    cwd,
    threadId,
    onuse,
    onerror,
  }: {
    text: string;
    cwd?: string;
    threadId?: string;
    onuse: (tidy: string, original: string) => void;
    onerror?: (e: unknown) => void;
  } = $props();

  let working = $state(false);
  let tidy = $state<string | null>(null);
  let from = $state("");
  let dismissed = $state("");

  const offer = $derived(text.length >= LONG && !text.trimStart().startsWith("/") && text !== dismissed);
  // A result belongs to the text it was made from; typing on drops it.
  const result = $derived(tidy !== null && from === text ? tidy : null);
  // The person may edit the written prompt here before using it.
  let edited = $state<string | null>(null);
  $effect(() => {
    void result;
    edited = null;
  });
  const shown = $derived(edited ?? result ?? "");

  /** Tidies now, whatever the length; the composer's + menu calls this. */
  export function tidyNow() {
    if (text.trim() && !working) run();
  }

  async function run() {
    working = true;
    from = text;
    try {
      tidy = (await api.tidy(text, cwd, { context: true, threadId })).text;
    } catch (e) {
      onerror?.(e);
    } finally {
      working = false;
    }
  }

  function use() {
    if (result === null) return;
    dismissed = shown;
    onuse(shown, from);
    tidy = null;
  }
</script>

{#if result !== null}
  <div class="tidy">
    <div class="head"><span>Prompt</span><span class="muted">{from.length} → {shown.length} chars</span></div>
    <textarea rows="10" value={shown} oninput={(e) => (edited = e.currentTarget.value)} aria-label="The written prompt, editable"></textarea>
    <div class="actions">
      <button class="btn primary" onclick={use}>Use prompt</button>
      <button class="btn" onclick={() => ((dismissed = text), (tidy = null))}>Keep mine</button>
    </div>
  </div>
{:else if offer || working}
  <div class="offer">
    {#if working}
      <span class="muted">Writing the prompt…</span>
    {:else}
      <span class="muted">Long dump · {text.length} chars</span>
      <button class="link" onclick={run}>Write a prompt</button>
      <button class="link quiet" onclick={() => (dismissed = text)}>Not now</button>
    {/if}
  </div>
{/if}

<style>
  .offer { display: flex; align-items: center; gap: 10px; font-size: 13px; margin: 0 0 8px; }
  .muted { color: var(--muted); }
  .link { color: var(--accent); font-size: 13px; padding: 1px 6px; border-radius: var(--r-sm); }
  .link:hover { background: var(--accent-bg); }
  .link.quiet { color: var(--muted); }
  .link.quiet:hover { color: var(--text); background: var(--surface-hover); }
  .tidy { background: var(--surface-2); border: 1px solid var(--line); border-radius: var(--r-lg); padding: 10px 12px; margin: 0 0 10px; }
  .head { display: flex; justify-content: space-between; font-size: 13px; color: var(--text-2); margin-bottom: 6px; }
  textarea { width: 100%; box-sizing: border-box; max-height: 320px; resize: vertical; background: transparent; border: 0; outline: 0; padding: 0; white-space: pre-wrap; font-family: inherit; font-size: 14px; line-height: 1.5; color: var(--text); }
  .actions { display: flex; gap: 6px; margin-top: 10px; }
</style>
