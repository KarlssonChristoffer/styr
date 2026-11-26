<!-- The steps card: the plan the agent keeps while it works. Grey check = done, the working grid = the step now, hollow ring = next. While the turn runs it sits above the live step and holds the one moving grid of the thread (the rows around it stand still); a click on its head folds it to the step now. When the turn is over it goes into the turn's fold and shows there as a record.
     Used by Transcript.svelte. -->
<script lang="ts">
  // The steps card: the plan the agent keeps while it works. Grey check = done, the working grid =
  // the step now, hollow ring = next. While the turn runs it sits above the live step and holds the
  // one moving grid of the thread (the rows around it stand still); a click on its head folds it to
  // the step now. When the turn is over it goes into the turn's fold and shows there as a record.
  import Icon from "./Icon.svelte";
  import WorkGrid from "./WorkGrid.svelte";
  import { prefs } from "../prefs.svelte";
  import { workShape } from "../workGrid";
  import { duration } from "../activity";
  import { stepsView, type PlanItem } from "../steps";

  let { plan, threadId, live = false }: { plan: PlanItem; threadId: string; live?: boolean } = $props();

  let shut = $state(false);
  const view = $derived(stepsView(plan, live ? 6 : 99));
  const total = $derived(plan.entries.length);
  const status = (n: number) => plan.entries[n].status;
  // The one grid goes on the step now; with none now (the plan is just made, or all done and the
  // answer is being written) it goes in the head, so a running card always holds exactly one.
  const inHead = $derived(live && view.now < 0);
  const current = $derived(view.now >= 0 ? view.now : Math.min(view.done, total - 1));
  const took = (n: number) => {
    const m = plan.marks?.[n];
    return m?.from != null && m.to != null && m.to - m.from >= 1000 ? duration(m.to - m.from) : "";
  };
  const shape = $derived(workShape(`${threadId}:steps`, prefs.workIndicator));
</script>

{#snippet mark(n: number)}
  {#if status(n) === "completed"}
    <span class="m done"><Icon name="check" size={10} /></span>
  {:else if status(n) === "in_progress" && live && n === view.now}
    <span class="m"><WorkGrid {shape} size={14} /></span>
  {:else if status(n) === "in_progress"}
    <span class="m half"><i></i></span>
  {:else}
    <span class="m todo"><i></i></span>
  {/if}
{/snippet}

<div class="steps" class:live role="group" aria-label="Steps">
  <button class="hd" data-use="thread.steps" onclick={() => live && (shut = !shut)} aria-expanded={!shut} disabled={!live} data-tip={live ? (shut ? "Show the steps" : "Fold to the step now") : undefined}>
    {#if inHead}<span class="m"><WorkGrid {shape} size={14} /></span>{/if}
    <span class="label">Steps</span>
    <span class="count">{view.done} of {total}</span>
    {#if live}<span class="chev" class:open={!shut}><Icon name="chevron" size={12} /></span>{/if}
  </button>
  {#if shut}
    <div class="r now">{@render mark(current)}<span class="t">{plan.entries[current].content}</span></div>
  {:else}
    {#if view.hidden}<div class="more">{view.hidden} earlier {view.hidden === 1 ? "step" : "steps"} done</div>{/if}
    {#each view.shown as n (n)}
      <div class="r {status(n)}">
        {@render mark(n)}<span class="t">{plan.entries[n].content}</span>{#if took(n)}<em>{took(n)}</em>{/if}
      </div>
    {/each}
  {/if}
</div>

<style>
  .steps { border: 1px solid var(--line); border-radius: var(--r-lg); background: var(--bg); padding: 8px 14px 8px; margin: 10px 0 6px; }
  .steps:not(.live) { background: transparent; margin: 8px 0; }
  .hd { display: flex; align-items: center; gap: 8px; width: 100%; padding: 0; margin: 0 0 3px; color: var(--muted); font-size: 12px; text-align: left; }
  .hd:disabled { cursor: default; }
  .hd .count { margin-left: auto; }
  .chev { display: grid; transition: transform var(--dur-fast) var(--ease); transform: rotate(-90deg); }
  .chev.open { transform: none; }
  .r { display: flex; gap: 8px; align-items: center; padding: 3px 0; font-size: 14px; color: var(--muted); transition: color var(--dur-move) var(--ease); }
  .r.in_progress { color: var(--text); }
  .r .t { min-width: 0; overflow-wrap: anywhere; }
  .r em { margin-left: auto; padding-left: 12px; font-style: normal; font-size: 12px; color: var(--muted); white-space: nowrap; }
  .more { color: var(--muted); font-size: 12px; padding: 1px 0 2px 25px; }
  .m { width: 16px; height: 16px; flex: none; display: grid; place-items: center; }
  .m.done { border-radius: 50%; background: var(--muted); color: var(--bg); }
  .m.todo i, .m.half i { width: 13px; height: 13px; border-radius: 50%; border: 1.5px solid var(--line-strong); }
  .m.half i { border-color: var(--muted); }
</style>
