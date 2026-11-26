<!-- One of several attempts at the same task: the attempts side by side, what each has changed, and picking the one whose work goes into the folder.
     Used by ThreadView.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { poll } from "../poll";
  // One of several attempts at the same task: the attempts side by side, what each has changed,
  // and picking the one whose work goes into the folder.
  import { api } from "../api";
  import type { AttemptGroup } from "../bindings/AttemptGroup";
  import type { Thread } from "../bindings/Thread";
  import { statusLabel } from "../format";
  import { hub } from "../state.svelte";
  import type { AttemptStart } from "../transcript";
  import Icon from "./Icon.svelte";

  let { thread, start }: { thread: Thread; start: AttemptStart } = $props();

  let group = $state<AttemptGroup | null>(null);
  // What the attempts are doing, so the numbers refresh as turns end.
  const statuses = $derived(start.threads.map((id) => hub.threads.find((t) => t.id === id)?.status ?? "").join());
  $effect(() => {
    void statuses;
    const id = thread.id;
    const load = () => quiet(api.attempts(id).then((g) => (group = g)), "attempts");
    load();
    const timer = poll(load, 15_000);
    return () => timer();
  });

  const model = (m: string | undefined) => (m ? m.replace(/^claude-/, "").replace(/\[.*\]$/, "") : "default");
  const busy = $derived(thread.status === "running" || thread.status === "waiting_on_you");
  let confirm = $state(false);
  let picking = $state(false);
  async function pick() {
    if (!confirm) {
      confirm = true;
      setTimeout(() => (confirm = false), 4000);
      return;
    }
    picking = true;
    try {
      group = await api.pickAttempt(thread.id);
    } catch (e) {
      hub.fail(e);
    } finally {
      picking = false;
      confirm = false;
    }
  }
</script>

{#if group && !group.done}
  <div class="attempts">
    <div class="head">
      <Icon name="branch" size={13} />
      <span>Attempt {start.n} of {start.of}, each in its own worktree</span>
      <button class="pick" class:armed={confirm} disabled={busy || picking} data-tip={busy ? "Wait for this attempt to finish its turn" : "Bring this attempt's work into the folder and archive the others"} onclick={pick}>
        {picking ? "Picking…" : confirm ? `Bring into ${group.cwd.split(/[\\/]/).pop()}? Click again` : "Pick this one"}
      </button>
    </div>
    {#each group.attempts as a (a.thread_id)}
      <button class="row" class:here={a.thread_id === thread.id} onclick={() => hub.open(a.thread_id)} data-tip={statusLabel[a.status]}>
        <span class="dot {a.status}"></span>
        <span class="n">{a.n}</span>
        <span class="model">{model(a.model)}</span>
        <span class="grow"></span>
        {#if a.files}
          <span class="stat">{a.files} {a.files === 1 ? "file" : "files"}</span>
          <span class="add">+{a.added}</span><span class="del">−{a.removed}</span>
        {:else}
          <span class="stat">no changes yet</span>
        {/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .attempts { background: var(--surface); border-radius: var(--r-md); padding: 6px 6px 6px; margin: 4px 0 12px; font-size: 13px; }
  .head { display: flex; align-items: center; gap: 8px; color: var(--text-3); padding: 2px 6px 6px; }
  .head span { flex: 1; }
  .pick { color: var(--accent); padding: 2px 8px; border-radius: var(--r-sm); font-size: 12px; }
  .pick:hover:not(:disabled) { background: var(--accent-bg); }
  .pick.armed { color: var(--amber); background: var(--amber-bg); }
  .pick:disabled { color: var(--muted); }
  .row { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 6px; border-radius: var(--r-sm); color: var(--text-3); text-align: left; }
  .row:hover { background: var(--surface-hover); }
  .row.here { background: var(--surface-hover); color: var(--text); }
  .n { color: var(--muted); font-variant-numeric: tabular-nums; width: 10px; }
  .model { font-family: var(--mono); font-size: 12px; }
  .grow { flex: 1; }
  .stat { color: var(--muted); }
  .add { color: var(--green); font-variant-numeric: tabular-nums; }
  .del { color: var(--red); font-variant-numeric: tabular-nums; }
</style>
