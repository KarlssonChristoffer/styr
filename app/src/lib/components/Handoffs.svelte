<!-- Settings > Handoffs: which model does which kind of task, in everyday chats and in dev threads. "The thread" means it does the task itself; a pick means only that engine and model may (`handoff.rs`). Hub runs plugin vetting and its own tidying with it; for the rest the agent is told to hand a side job over, and Hub refuses another engine.
     Used by Settings.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // Settings > Handoffs: which model does which kind of task, in everyday chats and in dev threads.
  // "The thread" means it does the task itself; a pick means only that engine and model may
  // (`handoff.rs`). Hub runs plugin vetting and its own tidying with it; for the rest the agent is
  // told to hand a side job over, and Hub refuses another engine.
  import { untrack } from "svelte";
  import type { HandoffPick } from "../bindings/HandoffPick";
  import { decode, encode, handoffs, loadHandoffs, setPick, TASKS } from "../handoffs.svelte";
  import { engines, harnessLabel, modelsFor } from "../providers.svelte";
  import Select, { type Option } from "./Select.svelte";

  $effect(() => {
    untrack(loadHandoffs);
  });

  const THREAD = "The thread";
  const nameOf = (p: HandoffPick) => {
    const c = modelsFor(p.engine)?.choices.find((m) => m.value === p.model);
    return `${harnessLabel(p.engine)}${p.model ? ` · ${c?.name ?? p.model}` : ""}`;
  };
  /** The engines this machine has, each with its models as last offered, grouped by harness. */
  const all = $derived<Option<string>[]>([
    { value: "", label: THREAD },
    ...engines.names.flatMap((engine) => {
      const group = harnessLabel(engine);
      const models = modelsFor(engine)?.choices ?? [];
      return [
        { value: encode({ engine, model: null }), label: group, hint: "Its own model", group },
        ...models.map((m) => ({ value: encode({ engine, model: m.value }), label: `${group} · ${m.name}`, group })),
      ];
    }),
  ]);
  /** The options, plus the current pick when this machine does not offer it (an engine it lacks). */
  function options(current: HandoffPick | null | undefined): Option<string>[] {
    const v = encode(current);
    return current && !all.some((o) => o.value === v) ? [...all, { value: v, label: nameOf(current) }] : all;
  }
  const prefs = $derived(handoffs.prefs);
</script>

{#if handoffs.has === false}
  <Quiet kind="newer" />
{:else}
  <div class="knob">
    <div class="th"><span>Task</span><span>In chats</span><span>In dev threads</span></div>
    {#each TASKS as t (t.id)}
      {@const row = prefs?.tasks[t.id]}
      <div class="tr">
        <span class="n" data-tip={t.hint}>{t.label}</span>
        {#each ["chat", "dev"] as const as side (side)}
          <Select
            size="small"
            class={row?.[side] ? "" : "def"}
            label={`${t.label} ${side === "chat" ? "in chats" : "in dev threads"}`}
            value={encode(row?.[side])}
            options={options(row?.[side])}
            disabled={!prefs}
            onchange={(v) => setPick(t.id, side, decode(v))}
          />
        {/each}
      </div>
    {/each}
    <div class="foot dim">Only what is picked may do the task. If it cannot, the task fails and you decide.</div>
  </div>
{/if}

<style>
  .knob { border: 1px solid var(--line-strong); border-radius: var(--r-lg); padding: 12px 14px; display: flex; flex-direction: column; gap: 2px; max-width: 640px; margin-top: 18px; }
  .th, .tr { display: grid; grid-template-columns: 170px 1fr 1fr; gap: 10px; align-items: center; }
  .th { padding: 4px 6px 6px; color: var(--muted); font-size: 12px; border-bottom: 1px solid var(--line); }
  .tr { padding: 6px; border-bottom: 1px solid var(--line); min-height: 42px; }
  .n { color: var(--text-2); }
  .tr :global(.def) { color: var(--muted); }
  .foot { padding: 8px 6px 2px; }
  .dim { color: var(--muted); font-size: 13px; }
</style>
