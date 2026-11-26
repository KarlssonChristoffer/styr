<!-- Why the window came back from Live mode's pages (`live.svelte.ts`): they broke or stopped answering. Says which pull request when the error points at its files; Try again goes live.
     Used by App.svelte. -->
<script lang="ts">
  // Why the window came back from Live mode's pages (`live.svelte.ts`): they broke or stopped
  // answering. Says which pull request when the error points at its files; Try again goes live.
  import { live } from "../live.svelte";
</script>

<div class="note" role="status">
  <div class="title">Back on the installed pages</div>
  <div class="why">{live.note}</div>
  <div class="actions">
    <button onclick={() => live.toggle()} disabled={!!live.going}>{live.going ? "Starting…" : "Try again"}</button>
    <button class="quiet" onclick={() => (live.note = null)}>Dismiss</button>
  </div>
</div>

<style>
  .note {
    position: absolute; right: 20px; bottom: 150px; z-index: 50; width: 360px; padding: 12px 14px;
    background: var(--surface-2); border: 1px solid var(--line-strong); border-left: 3px solid var(--amber);
    border-radius: var(--r-lg); box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45); font-size: 13px;
  }
  .title { font-weight: 600; color: var(--text); margin-bottom: 4px; }
  .why { color: var(--muted); margin-bottom: 10px; overflow-wrap: anywhere; }
  .actions { display: flex; gap: 14px; }
  .actions button { color: var(--text-2); font-size: 13px; }
  .actions button:hover { color: var(--text); }
  .actions .quiet { color: var(--muted); }
</style>
