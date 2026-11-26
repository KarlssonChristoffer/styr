<!-- A checkpoint between messages: a hairline with a small undo mark. Clicking the mark says what rolling back would change in one line, and a second click rolls back.
     Used by Transcript.svelte. -->
<script lang="ts">
  // A checkpoint between messages: a hairline with a small undo mark. Clicking the mark says
  // what rolling back would change in one line, and a second click rolls back.
  import { api } from "../api";
  import type { UndoPreview } from "../bindings/UndoPreview";
  import { clock } from "../format";
  import { pending } from "../menus.svelte";
  import Icon from "./Icon.svelte";

  let {
    threadId,
    to,
    ts,
    busy,
    hidden = false,
  }: {
    threadId: string;
    /** The snapshot to go back to. */
    to: string;
    ts?: number;
    /** The thread is working; it has to stop first. */
    busy: boolean;
    /** Only shows while asked, e.g. to undo a rollback. */
    hidden?: boolean;
  } = $props();

  const open = $derived(pending.undo?.thread === threadId && pending.undo.to === to);
  let preview = $state<UndoPreview | null>(null);
  let error = $state<string | null>(null);
  let working = $state(false);
  let el = $state<HTMLElement>();

  $effect(() => {
    preview = null;
    error = null;
    if (!open) return;
    el?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    api.undoPreview(threadId, to).then(
      (p) => (preview = p),
      (e) => (error = e instanceof Error ? e.message : String(e)),
    );
  });

  const toggle = () => (pending.undo = open ? null : { thread: threadId, to, label: "" });
  const nothing = $derived(!!preview && !preview.files.length && !preview.commits.length);
  const summary = $derived.by(() => {
    if (!preview) return "";
    const n = preview.files.length;
    const parts = [n ? `${n} ${n === 1 ? "file" : "files"}` : ""];
    if (preview.commits.length && !preview.kept_commits)
      parts.push(`${preview.commits.length} ${preview.commits.length === 1 ? "commit" : "commits"}`);
    return parts.filter(Boolean).join(" and ");
  });
  const detail = $derived(
    preview
      ? [
          ...preview.files.map((f) => `${f.action === "remove" ? "delete" : f.action === "recreate" ? "bring back" : "restore"}  ${f.path}`),
          ...(preview.kept_commits ? [`Commits stay: ${preview.kept_commits}`] : preview.commits.map((c) => `undo commit ${c}`)),
        ].join("\n")
      : "",
  );

  async function rollBack() {
    working = true;
    try {
      await api.undo(threadId, to);
      pending.undo = null;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      working = false;
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") pending.undo = null;
  }
</script>

<svelte:window onkeydown={open ? key : undefined} />

{#if !hidden || open}
  <div class="cp" class:open bind:this={el}>
    <button class="mark" data-tip={ts ? `Checkpoint ${clock(ts)} · roll back to here` : "Roll back to here"} onclick={toggle}>
      <Icon name="undo" size={11} />
    </button>
    {#if open}
      <span class="say">
        {#if error}<span class="err">{error}</span>
        {:else if !preview}Reading…
        {:else if nothing}Nothing changed since here.
        {:else}<span data-tip={detail}>Roll back {summary}{preview.kept_commits ? " (commits stay)" : ""}?</span>
          <button class="go" disabled={working || busy} data-tip={busy ? "Stop the thread first" : "Hub keeps how it is now, so this can be undone"} onclick={rollBack}>
            {working ? "Rolling back…" : busy ? "Stop first" : "Roll back"}
          </button>
        {/if}
      </span>
    {/if}
    <span class="rule"></span>
  </div>
{/if}

<style>
  .cp { display: flex; align-items: center; gap: 8px; height: 18px; margin: 10px 0 -6px; }
  .rule { flex: 1; height: 1px; background: var(--line); opacity: 0; transition: opacity 0.12s var(--ease); }
  .mark { display: grid; place-items: center; width: 18px; height: 18px; border-radius: 50%; color: var(--muted); opacity: 0.45; transition: opacity 0.12s var(--ease); }
  .cp:hover .rule, .cp:hover .mark, .cp.open .rule, .cp.open .mark, .mark:focus-visible { opacity: 1; }
  .mark:hover, .cp.open .mark { color: var(--text); background: var(--surface-hover); }
  .say { font-size: 12px; color: var(--text-3); white-space: nowrap; display: inline-flex; align-items: center; gap: 8px; }
  .err { color: var(--red); white-space: normal; }
  .go { color: var(--red); font-size: 12px; padding: 1px 8px; border-radius: var(--r-sm); background: var(--red-bg); }
  .go:disabled { opacity: 0.6; cursor: default; }
</style>
