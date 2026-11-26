<!-- Usage the way Claude Code shows it: plan limits first, then this thread and the others.
     Used by Composer.svelte, Settings.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // Usage the way Claude Code shows it: plan limits first, then this thread and the others.
  import { onDestroy, untrack } from "svelte";
  import Icon from "./Icon.svelte";
  import { api } from "../api";
  import type { UsageReport } from "../bindings/UsageReport";
  import type { ContextDetail } from "../bindings/ContextDetail";
  import { tokens, type Usage } from "../session";
  import { hub } from "../state.svelte";
  import { ago, resetOf, resets } from "../format";
  import { shownPlan } from "../plan";

  let {
    threadId,
    current,
    oncompact,
    engine,
    showThreads = true,
  }: { threadId?: string; current?: Usage; oncompact?: () => void; engine?: string; showThreads?: boolean } = $props();

  let report = $state<UsageReport | null>(cached);
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function load(refresh: boolean | "force", tries = 0) {
    try {
      report = cached = await api.usage(refresh);
      hub.setUsage(report);
      // Plan limits come from a short engine run; follow it until it is done.
      if (report.refreshing && tries < 60) timer = setTimeout(() => load(false, tries + 1), 1500);
    } catch (e) {
      hub.fail(e);
    }
  }
  // Only Claude's limits need the engine asked; the others are read from files each time.
  load(untrack(() => !engine || engine === "claude"));
  onDestroy(() => clearTimeout(timer));

  // What the thread's tokens went to, fetched when opened.
  let detail = $state<ContextDetail | null>(null);
  let detailOpen = $state(false);
  let detailLoading = $state(false);
  async function toggleDetail() {
    detailOpen = !detailOpen;
    if (!detailOpen || !threadId) return;
    detailLoading = true;
    try {
      detail = await api.contextDetail(threadId);
    } catch (e) {
      hub.fail(e);
    } finally {
      detailLoading = false;
    }
  }
  /** The sources of the thread's tokens, heaviest first: what was said, thinking, and each MCP server. */
  const sources = $derived.by(() => {
    if (!detail?.found) return [];
    const rows = [
      { name: "Conversation", hint: "Your messages and the replies", tokens: detail.conversation, calls: 0 },
      { name: "Thinking", hint: "The model's thinking", tokens: detail.thinking, calls: 0 },
      ...detail.servers.map((s) => ({
        name: s.server ? `MCP: ${s.server}` : s.name,
        hint: detail!.tools.filter((t) => (t.server ?? null) === (s.server ?? null)).slice(0, 6).map((t) => `${t.name.replace(/^mcp__[^_]+(?:_[^_]+)*?__/, "")} · ${t.calls}× · ${tokens(t.tokens)}`).join("\n"),
        tokens: s.tokens,
        calls: s.calls,
      })),
    ].filter((r) => r.tokens > 0);
    return rows.sort((a, b) => b.tokens - a.tokens);
  });
  const heaviest = $derived(Math.max(1, ...sources.map((s) => s.tokens)));

  const level = (pct: number) => (pct >= 95 ? "full" : pct >= 80 ? "warn" : "");
  // The last limits seen show until the engine has answered.
  const plan = $derived(report?.plan ?? hub.plan);
  const others = $derived(report?.others ?? hub.others);
  /** Another subscription's limits, when this panel is for one. */
  const other = $derived(engine && engine !== "claude" ? others.find((o) => o.engine === engine) ?? null : null);
  const contextPct = $derived(current && current.size ? (current.used / current.size) * 100 : 0);

  const threads = $derived.by(() => {
    const titles = new Map(hub.threads.map((t) => [t.id, t]));
    return (report?.threads ?? [])
      .filter((u) => u.thread_id !== threadId && titles.has(u.thread_id))
      .map((u) => ({ ...u, thread: titles.get(u.thread_id)! }))
      .sort((a, b) => Number(b.thread.status === "running") - Number(a.thread.status === "running") || b.used - a.used)
      .slice(0, 5);
  });
</script>

<script lang="ts" module>
  // Kept between openings so the panel shows the last numbers at once.
  let cached: UsageReport | null = null;
</script>

{#snippet checked(at: number | undefined, hint: string, onclick: () => void)}
  <button class="checked" data-tip={hint} {onclick}>
    {#if at}<span data-tip="Checked {ago(at) === "now" ? "just now" : `${ago(at)} ago`}">{ago(at)}</span>{/if}<Icon name="restart" size={12} />
  </button>
{/snippet}

<div class="usage-panel">
  {#if threadId}
    <div class="head"><span>This thread</span>{#if current?.cost}<span class="dim">${current.cost.amount.toFixed(2)}</span>{/if}</div>
    {#if current}
      <div class="limit">
        <div class="row"><span class="label">Context</span><span class="dim"></span><span class="pct">{tokens(current.used)} / {tokens(current.size)} · {Math.round(contextPct)}%</span></div>
        <div class="bar"><span class={level(contextPct)} style:width="{Math.max(contextPct, 1)}%"></span></div>
      </div>
      {#if contextPct >= 80}<div class="dim note">Getting full. Compact soon.</div>{/if}
    {/if}
    <button class="action" onclick={toggleDetail} aria-expanded={detailOpen}>
      <span>What used it</span><span class="dim">{detailOpen ? "Hide" : "Details ›"}</span>
    </button>
    {#if detailOpen}
      <div class="detail">
        {#if detailLoading && !detail}
          <Quiet kind="loading" />
        {:else if detail && !detail.found}
          <div class="dim note">{detail.note}</div>
        {:else if detail}
          {@const work = Math.max(0, detail.now - detail.start)}
          <div class="row sub"><span class="label">In context now</span><span class="pct">{tokens(detail.now)}</span></div>
          <div class="stack" data-tip="Setup {tokens(detail.start)} · work since {tokens(work)}">
            <span class="setup" style:width="{(detail.start / Math.max(1, detail.now)) * 100}%"></span>
            <span class="work" style:width="{(work / Math.max(1, detail.now)) * 100}%"></span>
          </div>
          <div class="legend">
            <span><i class="setup"></i>Setup {tokens(detail.start)}<span class="dim"> system prompt, tools, rules</span></span>
            <span><i class="work"></i>Work {tokens(work)}</span>
          </div>
          <div class="row sub"><span class="label">Spent over the thread</span><span class="dim">{detail.answers} replies{detail.compactions ? ` · compacted ${detail.compactions}×` : ""}</span></div>
          <div class="spent">
            <span data-tip="New input the model read">In {tokens(detail.input + detail.cache_write)}</span>
            <span data-tip="Input read again from the cache, cheap">Cache {tokens(detail.cache_read)}</span>
            <span data-tip="What the model wrote, thinking included">Out {tokens(detail.output)}</span>
          </div>
          {#if sources.length}
            <div class="row sub"><span class="label">By source</span><span class="dim" data-tip="Estimated from text size, about four characters a token">≈ tokens</span></div>
            {#each sources as s (s.name)}
              <div class="source" data-tip={s.hint || undefined}>
                <span class="label">{s.name}</span>
                {#if s.calls}<span class="dim">{s.calls}×</span>{/if}
                <span class="mini"><span style:width="{(s.tokens / heaviest) * 100}%"></span></span>
                <span class="pct">{tokens(s.tokens)}</span>
              </div>
            {/each}
            {#if detail.servers.some((s) => s.server)}
              <button class="action" onclick={() => hub.openSettings("extensions")}><span class="dim">Turn an MCP server off</span><span class="dim">Extensions ›</span></button>
            {/if}
          {/if}
        {/if}
      </div>
    {/if}
    {#if oncompact}
      <button class="action" onclick={oncompact}>Compact the conversation<span class="dim">frees context</span></button>
    {/if}
    <div class="sep"></div>
  {/if}

  {#if threadId}
    <!-- In a thread the plan is one line; the whole picture is in Settings > Account and usage. -->
    <button class="action" onclick={() => hub.openSettings("account")}>
      <span>Plan limits{#if plan} {#each plan.limits as l, i (l.label)}<span class="pct {level(l.percent)}" data-tip={l.label}>{i ? " · " : ""}{Math.round(l.percent)}%</span>{/each}{/if}</span>
      <span class="dim">All usage ›</span>
    </button>
  {:else if other}
  <div class="head">
    <span>Plan usage limits{shownPlan(other.engine, other.plan?.plan) ? ` · ${shownPlan(other.engine, other.plan?.plan)}` : ""}</span>
    {@render checked(other.plan?.checked_at, `Read again. ${other.label} updates its limits each time it answers`, () => load(false))}
  </div>
  {#if other.plan}
    {#each other.plan.limits as l (l.label)}
      {@const when = resetOf(l)}
      <div class="limit">
        <div class="row"><span class="label">{l.label}</span><span class="dim">{when ? `Resets ${when}` : ""}</span><span class="pct">{Math.round(l.percent)}%</span></div>
        <div class="bar"><span class={level(l.percent)} style:width="{Math.max(l.percent, 1)}%"></span></div>
      </div>
    {/each}
  {:else if other.note}
    <div class="dim note">{other.note}</div>
  {/if}
  {:else}
  <div class="head">
    <span>Plan usage limits{shownPlan("claude", plan?.plan) ? ` · ${shownPlan("claude", plan?.plan)}` : ""}</span>
    {#if !report || report.refreshing}<span class="dim">updating…</span>
    {:else}{@render checked(plan?.checked_at, "Ask Claude Code for your limits now", () => load("force"))}{/if}
  </div>
  {#if plan}
    {#each plan.limits as l (l.label)}
      <div class="limit">
        <div class="row"><span class="label">{l.label}</span><span class="dim">{l.resets ? `Resets ${resets(l.resets)}` : ""}</span><span class="pct">{Math.round(l.percent)}%</span></div>
        <div class="bar"><span class={level(l.percent)} style:width="{Math.max(l.percent, 1)}%"></span></div>
      </div>
    {/each}
  {:else if report?.plan_error}
    <div class="dim note">{report.plan_error}</div>
  {:else}
    <div class="dim note">Asking Claude Code for your limits…</div>
  {/if}
  {/if}

  {#if showThreads && threads.length && !other}
    <div class="sep"></div>
    <div class="head"><span>Other threads</span><span class="dim">context</span></div>
    {#each threads as t (t.thread_id)}
      <button class="thread" onclick={() => hub.open(t.thread_id)}>
        <span class="dot {t.thread.status}"></span>
        <span class="title">{t.thread.title}{#if threads.filter((x) => x.thread.title === t.thread.title).length > 1} <span class="dim">· {t.thread.cwd.split(/[\\/]/).filter(Boolean).at(-1) ?? ""}</span>{/if}</span>
        <span class="dim">{tokens(t.used)}{t.cost ? ` · $${t.cost.toFixed(2)}` : ""}</span>
      </button>
    {/each}
  {/if}
</div>

<style>
  .usage-panel { width: 340px; padding: 6px 6px 4px; font-size: 13px; }
  .head { display: flex; justify-content: space-between; color: var(--muted); padding: 2px 4px 6px; }
  .dim { color: var(--muted); }
  .checked { display: flex; align-items: center; gap: 4px; margin: -3px -4px 0 0; height: 22px; padding: 0 4px; border-radius: var(--r-sm); color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; }
  .checked:hover { background: var(--surface-hover); color: var(--text); }
  .note { padding: 0 4px 6px; }
  .limit { padding: 3px 4px 8px; }
  .row { display: flex; gap: 8px; align-items: baseline; margin-bottom: 4px; }
  .label { color: var(--text); flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row .dim { font-size: 12px; white-space: nowrap; }
  .pct { color: var(--text-2); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .bar { height: 4px; border-radius: 2px; background: var(--line-strong); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--accent); border-radius: 2px; }
  .bar span.warn { background: var(--amber); }
  .bar span.full { background: var(--red); }
  .pct.warn { color: var(--amber); }
  .pct.full { color: var(--red); }
  .sep { height: 1px; background: var(--line-strong); margin: 6px 4px 8px; }
  .action, .thread { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 4px; border-radius: var(--r-sm); text-align: left; color: var(--text-2); }
  .action { justify-content: space-between; }
  .action:hover, .thread:hover { background: var(--surface-hover); color: var(--text); }
  .detail { padding: 2px 0 4px; }
  .row.sub { padding: 6px 4px 0; margin-bottom: 4px; }
  .row.sub .label { color: var(--muted); }
  .stack { display: flex; height: 6px; margin: 0 4px; border-radius: 3px; overflow: hidden; background: var(--line-strong); }
  .stack .setup, .legend i.setup { background: var(--muted); }
  .stack .work, .legend i.work { background: var(--accent); }
  .legend { display: flex; gap: 12px; flex-wrap: wrap; padding: 4px 4px 2px; font-size: 12px; color: var(--text-2); }
  .legend i { display: inline-block; width: 7px; height: 7px; border-radius: 2px; margin-right: 4px; }
  .spent { display: flex; gap: 12px; padding: 0 4px 2px; font-size: 12px; color: var(--text-2); font-variant-numeric: tabular-nums; }
  .source { display: flex; align-items: center; gap: 8px; padding: 3px 4px; }
  .source .label { min-width: 0; }
  .mini { flex: none; width: 64px; height: 4px; border-radius: 2px; background: var(--line-strong); overflow: hidden; }
  .mini span { display: block; height: 100%; background: var(--text-2); }
  .source .pct { width: 38px; text-align: right; }
  .thread .title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
