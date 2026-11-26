<!-- A task Hub ran for this thread on the engine picked for it in Settings > Handoffs (say Customs vetting something the agent declared, on Hermes), as the same branch in the margin a side job has: who asked, who did it, what it was and how it ended. No thread behind it, so Open goes to where the result lives.
     Used by Transcript.svelte. -->
<script lang="ts">
  // A task Hub ran for this thread on the engine picked for it in Settings > Handoffs (say Customs
  // vetting something the agent declared, on Hermes), as the same branch in the margin a side job
  // has: who asked, who did it, what it was and how it ended. No thread behind it, so Open goes to
  // where the result lives.
  import { hub } from "../state.svelte";
  import { engineProvider, harnessLabel } from "../providers.svelte";
  import { duration } from "../activity";
  import type { TaskItem } from "../transcript";
  import Icon from "./Icon.svelte";
  import IconSwap from "./IconSwap.svelte";
  import ProviderMark from "./ProviderMark.svelte";

  let { item, parent: parentId }: { item: TaskItem; parent?: string } = $props();

  const TAGS: Record<string, string> = { vetting: "Customs", pictures: "Pictures", research: "Research", code_review: "Review", quick: "Quick", housekeeping: "Housekeeping" };
  const VERBS: Record<string, [string, string]> = { vetting: ["checking", "checked"] };
  const who = (engine: string) => ({ id: engineProvider(engine).id, name: harnessLabel(engine) });
  const p = $derived(who(item.engine));
  const parent = $derived.by(() => {
    const e = hub.threads.find((t) => parentId && t.id === parentId)?.engine;
    return e ? who(e) : null;
  });
  const verb = $derived(VERBS[item.task] ?? ["working on", "did"]);
  const verdict = $derived(item.state === "done" ? (item.verdict ?? "") : "");
  // The bracket's colour is the result's: green cleared, amber held, red refused or failed.
  const tone = $derived(item.state === "failed" || verdict === "refused" ? "bad" : verdict === "held" ? "need" : item.state === "done" ? "ok" : "");
  const took = $derived(item.ms ? duration(item.ms) : "");
  const words: Record<string, string> = { cleared: "Cleared", held: "Held", refused: "Refused" };
  const opens = $derived(item.task === "vetting");
</script>

<div class="task {tone}">
  <i class="top"></i><i class="bottom"></i>
  <div class="line">
    <Icon name="branch" size={12} />
    <span class="tag">{TAGS[item.task] ?? "Task"}</span>
    <span class="who">
      {#if parent}<ProviderMark id={parent.id} size={13} />{parent.name} asked {/if}{#if item.engine}<ProviderMark id={p.id} size={13} />{p.name}{:else}Hub{/if}{#if item.model}<span class="model"> · {item.model}</span>{/if}
    </span>
    <span class="grow"></span>
    {#if item.state === "working"}
      <span class="state"><span class="dot running"></span>{verb[0]}…</span>
    {:else if item.state === "failed"}
      <span class="state bad"><Icon name="x" size={12} />Failed</span>
    {:else}
      <span class="state {tone}"><IconSwap name={verdict === "cleared" ? "check" : verdict === "refused" ? "x" : "clock"} size={12} />{words[verdict] ?? "Done"}{took ? ` · ${took}` : ""}</span>
    {/if}
    {#if opens}
      <button class="link" onclick={() => hub.openSettings("customs")} data-tip="Open it in Customs"><Icon name="popout" size={11} />Open</button>
    {/if}
  </div>
  <div class="title">{#if item.state === "done"}{verb[1]}{" "}{/if}“{item.name}”</div>
  {#if item.reason && item.state !== "working"}<div class="why">{item.reason}</div>{/if}
</div>

<style>
  .task { --c: var(--p-sky); position: relative; margin: 12px 0 12px 20px; padding: 2px 0 2px 14px; font-size: 13px; min-width: 0; }
  .task.ok { --c: var(--green); }
  .task.need { --c: var(--amber); }
  .task.bad { --c: var(--red); }
  .task::before { content: ""; position: absolute; left: -14px; top: 9px; bottom: 9px; border-left: 1.5px solid var(--c); opacity: 0.55; }
  .top, .bottom { position: absolute; left: -14px; width: 16px; height: 10px; border: 0 solid var(--c); opacity: 0.55; }
  .top { top: 0; border-left-width: 1.5px; border-top-width: 1.5px; border-top-left-radius: var(--r-md); }
  .bottom { bottom: 0; border-left-width: 1.5px; border-bottom-width: 1.5px; border-bottom-left-radius: var(--r-md); }
  .line { display: flex; align-items: center; gap: 8px; color: var(--text-3); min-width: 0; flex-wrap: wrap; row-gap: 2px; }
  .line :global(svg:not(.mark)) { color: var(--c); flex: none; }
  .tag { color: var(--c); font-size: 12px; font-weight: 500; white-space: nowrap; }
  .who { display: inline-flex; align-items: center; gap: 4px; white-space: nowrap; }
  .model { color: var(--muted); }
  .grow { flex: 1; }
  .state { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 12px; white-space: nowrap; }
  .state.ok, .state.ok :global(svg) { color: var(--green); }
  .state.bad, .state.bad :global(svg) { color: var(--red); }
  .state.need, .state.need :global(svg) { color: var(--amber); }
  .title { color: var(--text-2); margin: 3px 0 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .why { color: var(--muted); font-size: 12px; margin-top: 2px; }
  .line .dot { margin: 0 1px; }
  .link { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 12px; padding: 1px 6px; border-radius: 6px; flex: none; }
  .link:hover { color: var(--accent); background: var(--accent-bg); }
  .line .link :global(svg) { color: inherit; }
</style>
