<!-- The chain of a Worker run's steps: done ones plain, the one going on lit, the rest dim. A step the run went through more than once says how often. Used in the inbox row and the run card.
     Used by ActivityList.svelte, WorkerRun.svelte. -->
<script lang="ts">
  // The chain of a Worker run's steps: done ones plain, the one going on lit, the rest dim. A step
  // the run went through more than once says how often. Used in the inbox row and the run card.
  import type { WorkerRun } from "../bindings/WorkerRun";

  let { run, small = false }: { run: WorkerRun; small?: boolean } = $props();
  const tone = $derived(run.paused ? "paused" : run.state === "running" ? "working" : run.state);
</script>

<span class="chain is-{tone}" class:small>
  {#each run.steps as s, i (s.id)}
    {#if i}<span class="sep">›</span>{/if}
    <span class="step {s.state}">{s.label}{#if s.visits > 1}<span class="n">{s.visits}</span>{/if}</span>
  {/each}
</span>

<style>
  .chain { display: inline-flex; align-items: baseline; gap: 4px; flex-wrap: wrap; font-size: 12px; min-width: 0; }
  .chain.small { font-size: 11px; flex-wrap: nowrap; overflow: hidden; }
  .sep { color: var(--muted); opacity: 0.6; }
  .step { color: var(--muted); white-space: nowrap; }
  .step.todo { opacity: 0.55; }
  .step.done { color: var(--text-3); }
  .step.now { color: var(--accent); }
  .is-paused .step.now { color: var(--amber); }
  .is-stopped .step.now { color: var(--red); }
  .is-done .step.done { color: var(--green); }
  .n { font-size: 0.85em; margin-left: 1px; vertical-align: super; opacity: 0.8; }
</style>
