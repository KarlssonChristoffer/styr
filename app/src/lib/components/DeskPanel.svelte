<!-- The right panel beside a project's desk: the project's stage now, and its threads by what they need, so you see the whole project while you talk to the desk. Hover a thread to peek at where it stands and steer it without leaving the desk; click it to open it.
     Used by App.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { poll } from "../poll";
  // The right panel beside a project's desk: the project's stage now, and its threads by what
  // they need, so you see the whole project while you talk to the desk. Hover a thread to peek
  // at where it stands and steer it without leaving the desk; click it to open it.
  import { api } from "../api";
  import type { FocusSummary } from "../bindings/FocusSummary";
  import type { Output } from "../bindings/Output";
  import type { Thread } from "../bindings/Thread";
  import { ago, clock, folderName, threadProject } from "../format";
  import { menu } from "../menu.svelte";
  import { threadMenu } from "../menus.svelte";
  import { openUrl } from "../native";
  import { hub, unread } from "../state.svelte";
  import { folded } from "../transcript";
  import Icon from "./Icon.svelte";

  let { desk }: { desk: Thread } = $props();

  const folder = $derived(threadProject(desk));
  const host = $derived(desk.host && desk.host !== hub.localHost ? desk.host : null);
  const name = $derived(hub.projectName(folder, host));

  // The project's threads, the desk and side jobs left out.
  const mine = $derived(
    hub.threads.filter((t) => !t.archived && !t.parent && t.id !== desk.id && threadProject(t) === folder && (t.host ?? null) === (desk.host ?? null)),
  );
  const needs = $derived(mine.filter((t) => t.status === "waiting_on_you" || t.status === "error").sort((a, b) => (b.returned_at ?? 0) - (a.returned_at ?? 0)));
  const working = $derived(mine.filter((t) => t.status === "running" || t.status === "in_terminal").sort((a, b) => (b.sent_at ?? b.updated_at) - (a.sent_at ?? a.updated_at)));
  const rest = $derived(
    mine.filter((t) => t.status === "idle" && !t.settled_at).sort((a, b) => (b.returned_at ?? b.updated_at) - (a.returned_at ?? a.updated_at)),
  );
  let all = $state(false);
  const shown = $derived(all ? rest : rest.slice(0, 8));

  // The stage now, from the project's docs/FOCUS.md, read again as its threads change.
  let stage = $state<FocusSummary | null>(null);
  $effect(() => {
    const f = folder;
    void mine.map((t) => t.status).join();
    if (host) return;
    quiet(api.focusSummaries([f]).then((s) => (stage = s[f] ?? null)), "focus summaries");
  });

  // The latest things the project's threads made, read again as its threads change.
  let made = $state<Output[]>([]);
  $effect(() => {
    const f = folder;
    void mine.map((t) => t.status).join();
    quiet(api.projectOutputs(f, host).then((o) => (made = o.slice(0, 4))), "project outputs");
  });
  function library() {
    hub.openProject(folder, host);
    hub.projectTab = "library";
  }

  // A tick a minute keeps the times fresh.
  let now = $state(Date.now());
  $effect(() => {
    const timer = poll(() => (now = Date.now()), 60_000);
    return () => timer();
  });

  // Peek: what a thread said last, and a box to steer it.
  let peek = $state<{ id: string; top: number } | null>(null);
  let steer = $state("");
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  const peeked = $derived(peek ? hub.threads.find((t) => t.id === peek!.id) : undefined);
  const peekLine = $derived.by(() => {
    if (!peeked) return "";
    const items = folded(hub.events[peeked.id] ?? []);
    const said = items.findLast((i) => i.kind === "assistant");
    if (!said || said.kind !== "assistant") return "";
    const text = said.text.trim();
    return text.length > 320 ? `…${text.slice(-320).trimStart()}` : text;
  });
  const peekPlan = $derived.by(() => {
    if (!peeked) return null;
    const plan = folded(hub.events[peeked.id] ?? []).findLast((i) => i.kind === "plan");
    if (!plan || plan.kind !== "plan" || !plan.entries.length) return null;
    return { done: plan.entries.filter((e) => e.status === "completed").length, of: plan.entries.length };
  });

  function show(t: Thread, e: MouseEvent) {
    clearTimeout(hideTimer);
    hub.follow(t.id);
    const row = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const zoom = Number(document.documentElement.style.zoom) || 1;
    if (peek?.id !== t.id) steer = "";
    peek = { id: t.id, top: row.top / zoom };
  }
  function hide() {
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => !steer && (peek = null), 260);
  }
  function keep() {
    clearTimeout(hideTimer);
  }
  async function sendSteer(e: Event) {
    e.preventDefault();
    if (!peeked || !steer.trim()) return;
    await hub.send(peeked.id, steer.trim());
    steer = "";
    peek = null;
  }

  const when = (t: Thread) => ago(t.returned_at ?? t.updated_at, now);
</script>

{#snippet row(t: Thread, sub?: string)}
  {@const pr = hub.prs[t.id]}
  <button class="row" class:on={peek?.id === t.id} onclick={() => hub.open(t.id)} onmouseenter={(e) => show(t, e)} onmouseleave={hide} use:menu={() => threadMenu(t)}>
    <span class="dot {t.status}" class:unread={unread(t)}></span>
    <span class="what">
      <span class="t ellipsis">{t.title}</span>
      {#if sub}<span class="sub ellipsis">{sub}</span>{/if}
    </span>
    {#if pr}<span class="pr {pr.state}" role="link" tabindex="-1" data-tip="{pr.title} ({pr.state})" onclick={(e) => { e.stopPropagation(); openUrl(pr.url); }} onkeydown={(e) => { if (e.key === "Enter") { e.stopPropagation(); openUrl(pr.url); } }}><Icon name="pr" size={11} />{pr.number}</span>{/if}
    <span class="when">{t.status === "running" ? "now" : when(t)}</span>
  </button>
{/snippet}

<div class="panel">
  <header>
    <button class="name" onclick={() => hub.openProject(folder, host)} data-tip="Open the project page"><Icon name="folder" size={14} /><span class="ellipsis">{name}</span></button>
    {#if stage?.stage}
      <div class="stage" data-tip="{stage.done} of {stage.of} delivered{stage.to_test ? `, ${stage.to_test} to test` : ''}">
        <span class="ellipsis">Stage {stage.n} · {stage.stage}</span>
        {#if stage.of}
          <span class="bar">{#each Array(Math.min(stage.of, 12)) as _, i}<i class:d={i < stage.done}></i>{/each}</span>
        {/if}
      </div>
      {#if stage.to_test}<button class="test" onclick={() => hub.openProject(folder, host)}>{stage.to_test} to test by hand</button>{/if}
    {:else if stage}
      <div class="stage">Every stage delivered</div>
    {/if}
  </header>

  {#if made.length}
    <h4><Icon name="star" size={11} />Made lately<span class="rule"></span><button class="all" onclick={library}>Library</button></h4>
    {#each made as o (o.id)}
      <button class="made" onclick={library} data-tip={o.places[0]?.detail ?? ""}>
        {#if o.live}<i class="live"></i>{/if}<span class="t ellipsis">{o.title}</span><span class="when">{o.live ? (o.places[0]?.label ?? "live") : ago(o.changed_at, now)}</span>
      </button>
    {/each}
  {/if}

  {#if needs.length}
    <h4><span class="dot waiting_on_you"></span>Needs you <span class="n">{needs.length}</span><span class="rule"></span></h4>
    {#each needs as t (t.id)}{@render row(t, t.status === "error" ? "Failed" : undefined)}{/each}
  {/if}
  {#if working.length}
    <h4><span class="dot running"></span>Working <span class="n">{working.length}</span><span class="rule"></span></h4>
    {#each working as t (t.id)}{@render row(t)}{/each}
  {/if}
  <h4><span class="dot idle"></span>Idle <span class="n">{rest.length}</span><span class="rule"></span></h4>
  {#each shown as t (t.id)}{@render row(t)}{/each}
  {#if rest.length > shown.length}<button class="more" onclick={() => (all = true)}>Show {rest.length - shown.length} more</button>{/if}
  {#if !mine.length}<p class="none">No threads in {folderName(folder)} yet. Tell the desk what to do and it starts them.</p>{/if}
</div>

{#if peek && peeked}
  <div role="presentation" class="peek" style:top="{Math.max(12, Math.min(peek.top - 8, innerHeight - 260))}px" onmouseenter={keep} onmouseleave={hide}>
    <div class="ph"><span class="dot {peeked.status}"></span><b class="ellipsis">{peeked.title}</b><span class="m">{peeked.engine} · {peeked.status === "running" ? "working" : when(peeked)}</span></div>
    {#if peekPlan || hub.prs[peeked.id]}
      <div class="facts">
        {#if peekPlan}<span class="plan">{#each Array(Math.min(peekPlan.of, 10)) as _, i}<i class:d={i < peekPlan.done}></i>{/each}{peekPlan.done}/{peekPlan.of}</span>{/if}
        {#if hub.prs[peeked.id]}<span class="pr {hub.prs[peeked.id].state}"><Icon name="pr" size={11} />#{hub.prs[peeked.id].number} {hub.prs[peeked.id].state}</span>{/if}
      </div>
    {/if}
    {#if peekLine}<p class="said">{peekLine}</p>{:else}<p class="said m">Nothing said yet{peeked.returned_at ? ` · back at ${clock(peeked.returned_at)}` : ""}.</p>{/if}
    <form onsubmit={sendSteer}>
      <input bind:value={steer} placeholder={peeked.status === "running" ? "Steer it: goes in as its next message" : "Tell it something"} />
    </form>
    <div class="acts">
      <button class="btn small" onclick={() => hub.open(peeked.id)}>Open</button>
      {#if peeked.status === "running"}<button class="btn small" onclick={() => api.cancel(peeked.id).catch((e) => hub.fail(e))}>Stop</button>{/if}
    </div>
  </div>
{/if}

<style>
  .panel { padding: 14px 16px 16px; overflow-y: auto; min-height: 0; flex: 1; }
  header { display: flex; flex-direction: column; gap: 6px; padding-bottom: 6px; }
  .name { display: inline-flex; align-items: center; gap: 8px; color: var(--text); font-size: 14px; font-weight: 500; min-width: 0; text-align: left; }
  .name:hover span { text-decoration: underline; text-underline-offset: 3px; text-decoration-color: var(--line-strong); }
  .stage { display: flex; align-items: center; gap: 10px; color: var(--text-3); font-size: 13px; min-width: 0; }
  .stage .ellipsis { min-width: 0; }
  .bar { display: flex; gap: 3px; flex: none; margin-left: auto; }
  .bar i { width: 14px; height: 4px; border-radius: 2px; background: var(--line-strong); }
  .bar i.d { background: var(--text-3); }
  .test { align-self: flex-start; color: var(--amber); font-size: 12px; }
  .test:hover { text-decoration: underline; text-underline-offset: 3px; }
  h4 { display: flex; align-items: center; gap: 8px; margin: 16px 0 4px; color: var(--muted); font-size: 12px; font-weight: 500; }
  h4 .n { color: var(--text-3); }
  h4 .rule { flex: 1; height: 1px; background: var(--line); }
  .row { display: flex; align-items: center; gap: 8px; width: 100%; padding: 6px 8px; margin: 0 -8px; width: calc(100% + 16px); border-radius: var(--r-md); text-align: left; color: var(--text-2); font-size: 13px; }
  .row:hover, .row.on { background: var(--surface-hover); color: var(--text); }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .sub { color: var(--muted); font-size: 12px; }
  .when { flex: none; color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; }
  .pr { flex: none; display: inline-flex; align-items: center; gap: 3px; font-size: 12px; color: var(--text-3); }
  .pr.open { color: var(--green); }
  .pr.merged { color: var(--accent); }
  .pr.closed { color: var(--red); }
  .all { color: var(--muted); font-size: 12px; }
  .all:hover { color: var(--text); }
  .made { display: flex; align-items: center; gap: 8px; width: calc(100% + 16px); margin: 0 -8px; padding: 4px 8px; border-radius: var(--r-md); text-align: left; color: var(--text-3); font-size: 13px; }
  .made:hover { background: var(--surface-hover); color: var(--text); }
  .made .t { flex: 1; min-width: 0; }
  .made .live { width: 7px; height: 7px; border-radius: 50%; flex: none; background: var(--green); box-shadow: 0 0 0 3px var(--green-bg); }
  .more { color: var(--muted); font-size: 12px; margin-top: 4px; }
  .more:hover { color: var(--text); }
  .none { color: var(--muted); font-size: 13px; line-height: 1.5; }
  .peek {
    position: fixed; right: calc(var(--panel-w) + 10px); width: 360px; z-index: 60; display: flex; flex-direction: column; gap: 8px;
    padding: 12px 12px; border-radius: var(--r-lg); background: var(--surface); box-shadow: inset 0 0 0 1px var(--line-strong), 0 18px 50px rgba(0, 0, 0, 0.45);
  }
  .ph { display: flex; align-items: center; gap: 8px; min-width: 0; font-size: 14px; }
  .ph b { font-weight: 500; min-width: 0; flex: 1; }
  .ph .m { flex: none; color: var(--muted); font-size: 12px; }
  .facts { display: flex; align-items: center; gap: 12px; font-size: 12px; color: var(--text-3); }
  .plan { display: inline-flex; align-items: center; gap: 2px; }
  .plan i { width: 8px; height: 4px; border-radius: 2px; background: var(--line-strong); margin-right: 1px; }
  .plan i.d { background: var(--green); }
  .said { margin: 0; padding-left: 10px; border-left: 1px solid var(--line-strong); color: var(--text-2); font-size: 13px; line-height: 1.5; white-space: pre-wrap; word-break: break-word; max-height: 140px; overflow: hidden; }
  .said.m { color: var(--muted); }
  .peek input { width: 100%; background: var(--bg); color: var(--text); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 6px 8px; font: inherit; font-size: 13px; }
  .peek input:focus { outline: none; border-color: var(--accent); }
  .acts { display: flex; gap: 6px; }
  .ellipsis { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
