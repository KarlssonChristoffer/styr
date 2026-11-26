<!-- Offers to keep a rule said in a message ("always…", "never…") in the folder's CLAUDE.md.
     Used by Transcript.svelte. -->
<script lang="ts">
  import { autofocus } from "../autofocus";
  // Offers to keep a rule said in a message ("always…", "never…") in the folder's CLAUDE.md.
  import { api } from "../api";
  import { hub } from "../state.svelte";

  let { threadId, seq, rule, saved }: { threadId: string; seq: number; rule: string; saved?: string } = $props();

  let editing = $state(false);
  let dismissed = $state(false);
  let draft = $state("");
  let busy = $state(false);

  function start() {
    draft = rule;
    editing = true;
  }

  async function save() {
    if (!draft.trim()) return;
    busy = true;
    try {
      await api.saveRule(threadId, draft.trim(), seq);
      editing = false;
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = false;
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Enter") save();
    else if (e.key === "Escape") editing = false;
  }

</script>

{#if saved}
  <span class="rule done" data-tip={saved}>Saved as a rule in CLAUDE.md</span>
{:else if editing}
  <span class="rule edit">
    <input bind:value={draft} onkeydown={key} use:autofocus disabled={busy} spellcheck="false" />
    <button class="link" onclick={save} disabled={busy}>Save to CLAUDE.md</button>
    <button class="link quiet" onclick={() => (editing = false)}>Cancel</button>
  </span>
{:else if !dismissed}
  <span class="rule">
    <button class="link" onclick={start} data-tip={rule}>Keep as a rule?</button>
    <button class="link quiet" onclick={() => (dismissed = true)} data-tip="Not a rule">×</button>
  </span>
{/if}

<style>
  .rule { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: var(--muted); }
  .done { padding-right: 4px; }
  .edit { width: min(560px, 80%); }
  input {
    flex: 1; min-width: 0; background: var(--surface); border: 1px solid var(--line-strong); border-radius: var(--r-md);
    padding: 4px 8px; font-size: 13px; color: var(--text); outline: none;
  }
  .link { color: var(--accent); font-size: 12px; padding: 1px 6px; border-radius: var(--r-sm); white-space: nowrap; }
  .link:hover { background: var(--accent-bg); }
  .link.quiet { color: var(--muted); }
  .link.quiet:hover { color: var(--text); background: var(--surface-hover); }
</style>
