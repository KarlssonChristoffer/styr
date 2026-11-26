<!-- A run of one of your Workers (Knegarn) in its thread: the chain of steps with the one going on, the round, the agents at work, and every line of the run folded by round. The buttons are the run's own controls from the tool's hub.toml; a dangerous one asks twice, and one that widens a budget says pressing it is an authorization.
     Used by Transcript.svelte. -->
<script lang="ts">
  // A run of one of your Workers (Knegarn) in its thread: the chain of steps with the one going on,
  // the round, the agents at work, and every line of the run folded by round. The buttons are the
  // run's own controls from the tool's hub.toml; a dangerous one asks twice, and one that widens a
  // budget says pressing it is an authorization.
  import { api } from "../api";
  import type { WorkerControl } from "../bindings/WorkerControl";
  import type { WorkerLine } from "../bindings/WorkerLine";
  import type { WorkerRun } from "../bindings/WorkerRun";
  import { clock } from "../format";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import WorkerChain from "./WorkerChain.svelte";

  let { run, lines, ts, threadId }: { run: WorkerRun; lines: WorkerLine[]; ts: number; threadId: string } = $props();

  let open = $state(false);
  let busy = $state<string | null>(null);
  let asking = $state<WorkerControl | null>(null);
  let inputs = $state<Record<string, string>>({});
  let sure = $state<string | null>(null);
  let said = $state<string | null>(null);

  const word = $derived(run.paused ? "paused" : run.state === "running" ? "working" : run.state);
  // The lines by round, newest round last; a line with no round joins the one before.
  const rounds = $derived.by(() => {
    const out: { round: number | null; lines: WorkerLine[] }[] = [];
    for (const line of lines) {
      const r = line.round ?? out.at(-1)?.round ?? null;
      if (out.at(-1)?.round === r) out.at(-1)!.lines.push(line);
      else out.push({ round: r, lines: [line] });
    }
    return out;
  });
  const shown = (l: WorkerLine) => l.text || l.kind.replaceAll("_", " ");

  async function press(c: WorkerControl) {
    said = null;
    if (c.ask.length && asking?.id !== c.id) {
      asking = c;
      inputs = Object.fromEntries(c.ask.map((a) => [a, inputs[a] ?? ""]));
      return;
    }
    if (c.danger && sure !== c.id) {
      sure = c.id;
      setTimeout(() => sure === c.id && (sure = null), 4000);
      return;
    }
    busy = c.id;
    try {
      const ran = await api.workerControl({ thread_id: threadId, control: c.id, inputs: c.ask.length ? inputs : {} });
      if (!ran.ok) said = ran.output || `${c.label} did not work.`;
      asking = null;
      sure = null;
    } catch (e) {
      said = e instanceof Error ? e.message : String(e);
    } finally {
      busy = null;
    }
  }
</script>

<div class="run is-{word}">
  <div class="head">
    <Icon name="workers" size={13} />
    <span class="tool">{run.tool}</span>
    <span class="title ellipsis" data-tip={run.title}>{run.title}</span>
    <span class="word">{word}{run.round ? ` · round ${run.round}` : ""}</span>
    <span class="when">{clock(ts)}</span>
  </div>
  <WorkerChain {run} />
  {#if run.agents.length}
    <div class="agents">
      {#each run.agents as a (a.role)}
        <span class="agent" data-tip={a.last ?? ""}><span class="role">{a.role}</span>{a.calls > 1 ? ` ×${a.calls}` : ""}{#if a.model}<span class="model mono">{a.model}</span>{/if}</span>
      {/each}
      {#if run.cost != null}<span class="cost">≈ ${run.cost.toFixed(2)}</span>{/if}
    </div>
  {/if}
  {#if run.state !== "running" && run.said}<div class="said" class:bad={run.state === "stopped"}>{run.said.replaceAll("_", " ")}</div>{/if}
  <div class="foot">
    {#if lines.length}
      <button class="link" onclick={() => (open = !open)}><Icon name="chevron" size={11} />{open ? "Hide" : "Show"} {lines.length} {lines.length === 1 ? "step" : "steps"}</button>
    {/if}
    <span class="grow"></span>
    {#if run.link}{@const link = run.link}<button class="btn small" onclick={() => openUrl(link).catch((e) => hub.fail(e))}><Icon name={link.includes("/pull/") ? "pr" : "external"} size={12} />{link.includes("/pull/") ? "Pull request" : "Open"}</button>{/if}
    {#each run.controls as c (c.id)}
      <button class="btn small" class:danger={c.danger} disabled={!!busy || !hub.connected} data-tip={c.about ?? ""} onclick={() => press(c)}>
        {#if c.id === "pause"}<Icon name="pause" size={12} />{:else if c.id === "resume"}<Icon name="play" size={12} />{:else if c.id === "stop"}<Icon name="stop" size={12} />{/if}
        {busy === c.id ? "…" : sure === c.id ? `${c.label}? Press again` : c.label}
      </button>
    {/each}
  </div>
  {#if asking}
    {@const c = asking}
    <form class="ask" onsubmit={(e) => { e.preventDefault(); press(c); }}>
      {#each c.ask as name (name)}
        <label><span>{name}</span><input class="mono" bind:value={inputs[name]} spellcheck="false" /></label>
      {/each}
      {#if c.authorizes}<p class="hint">Pressing {c.label} authorizes it: the run may spend more than it was given.</p>{/if}
      <div class="row">
        <button type="button" class="btn small" onclick={() => (asking = null)}>Cancel</button>
        <button type="submit" class="btn small primary" disabled={!!busy || c.ask.some((n) => !inputs[n]?.trim())}>{c.label}</button>
      </div>
    </form>
  {/if}
  {#if said}<p class="error">{said}</p>{/if}
  {#if open}
    <div class="lines">
      {#each rounds as r, i (i)}
        {#if r.round}<div class="round">Round {r.round}</div>{/if}
        {#each r.lines as l (l.seq)}
          <div class="line"><span class="step">{l.step ? (run.steps.find((s) => s.id === l.step)?.label ?? l.step) : ""}</span><span class="agentname">{l.agent ?? ""}</span><span class="text">{shown(l)}</span></div>
        {/each}
      {/each}
    </div>
  {/if}
</div>

<style>
  .run { border: 1px solid var(--line); border-radius: var(--r-md); background: var(--surface); padding: 10px 12px; margin: 6px 0; display: flex; flex-direction: column; gap: 8px; font-size: 13px; }
  .head { display: flex; align-items: center; gap: 6px; color: var(--muted); min-width: 0; }
  .tool { color: var(--text-3); flex: none; }
  .title { color: var(--text); flex: 1; min-width: 0; }
  .word { flex: none; }
  .is-working .word { color: var(--accent); }
  .is-paused .word { color: var(--amber); }
  .is-done .word { color: var(--green); }
  .is-stopped .word { color: var(--red); }
  .when { flex: none; color: var(--muted); font-size: 12px; }
  .agents { display: flex; flex-wrap: wrap; gap: 4px 10px; color: var(--text-3); }
  .agent { display: inline-flex; align-items: baseline; gap: 4px; }
  .role { color: var(--text-2); }
  .model { color: var(--muted); font-size: 11px; }
  .cost { color: var(--muted); margin-left: auto; }
  .said { color: var(--text-3); }
  .said.bad { color: var(--red); }
  .foot { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .grow { flex: 1; }
  .link { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 12px; }
  .link:hover { color: var(--text-2); }
  .btn.small { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; padding: 2px 8px; }
  .btn.danger { color: var(--red); }
  .ask { display: flex; flex-direction: column; gap: 6px; border-top: 1px solid var(--line); padding-top: 8px; }
  .ask label { display: flex; align-items: center; gap: 8px; color: var(--text-3); }
  .ask label span { min-width: 70px; }
  .ask input { flex: 1; background: var(--bg); border: 1px solid var(--line); border-radius: var(--r-md); outline: none; padding: 4px 8px; color: var(--text); font-size: 12px; }
  .ask .row { display: flex; justify-content: flex-end; gap: 6px; }
  .hint { color: var(--muted); font-size: 12px; margin: 0; }
  .error { color: var(--red); font-size: 12px; margin: 0; }
  .lines { border-top: 1px solid var(--line); padding-top: 6px; display: flex; flex-direction: column; gap: 2px; max-height: 360px; overflow-y: auto; }
  .round { color: var(--muted); font-size: 12px; margin-top: 6px; }
  .line { display: grid; grid-template-columns: 76px 84px 1fr; gap: 8px; color: var(--text-3); line-height: 1.5; }
  .step { color: var(--muted); }
  .agentname { color: var(--text-3); }
  .text { color: var(--text-2); overflow-wrap: anywhere; }
</style>
