<!-- The day picked in the activity map: the threads, Claude Code sessions, GitHub and commits of that day, and a summary written on request.
     Used by CardBody.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // The day picked in the activity map: the threads, Claude Code sessions, GitHub and commits
  // of that day, and a summary written on request.
  import { api } from "../api";
  import type { ActivityDetail } from "../bindings/ActivityDetail";
  import { clock, folderName, shortVersion } from "../format";
  import { dateKey, overview } from "../overview.svelte";
  import { openRepo } from "../refs";
  import { hub, unread } from "../state.svelte";
  import Markdown from "./Markdown.svelte";
  import Ref from "./Ref.svelte";

  const tz = -new Date().getTimezoneOffset();

  let detail = $state<ActivityDetail | null>(null);
  let summarizing = $state(false);
  const selected = $derived(overview.day);
  const today = dateKey(new Date());
  /** Whether the map lit the day: something happened. */
  const lit = $derived(!!selected && (overview.levels[selected] ?? 0) > 0);

  // The day's written entry shows at once; the threads, sessions and commits behind it are
  // worked out only when asked for.
  let report = $state<{ date: string; text: string | null } | null>(null);
  let showDetail = $state(false);
  let detailLoading = $state(false);

  $effect(() => {
    const date = overview.day;
    detail = null;
    report = null;
    showDetail = false;
    if (!date) return;
    api.devlogDay(date).then(
      (d) => {
        if (overview.day === date) report = { date, text: d?.summary ?? null };
      },
      (e) => hub.fail(e),
    );
  });

  async function loadDetail() {
    showDetail = !showDetail;
    if (!showDetail || detail?.date === selected || !selected) return;
    const date = selected;
    detailLoading = true;
    try {
      const d = await api.activityDay(date, tz);
      if (overview.day === date) detail = d;
    } catch (e) {
      hub.fail(e);
    } finally {
      detailLoading = false;
    }
  }

  async function summarize() {
    if (!selected) return;
    const date = selected;
    summarizing = true;
    try {
      const { text } = await api.summarizeDay(date, tz);
      if (overview.day === date) report = { date, text };
    } catch (e) {
      hub.fail(e);
    } finally {
      summarizing = false;
    }
  }

  const tokens = (n: number) =>
    n >= 1e9 ? `${(n / 1e9).toFixed(1)}B` : n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}k` : `${n}`;
  const commitsByRepo = $derived.by(() => {
    const groups = new Map<string, ActivityDetail["commits"]>();
    for (const c of detail?.commits ?? []) groups.set(c.repo, [...(groups.get(c.repo) ?? []), c]);
    return [...groups];
  });
  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
  const empty = (d: ActivityDetail) => !d.threads.length && !d.commits.length && !d.sessions.length && !d.github.length && !d.pulls.length;
  const open = (url: string) => api.openUrl(url).catch((e) => hub.fail(e));
</script>

{#if !selected}
  <p class="quiet">Pick a day in the activity map.</p>
{:else}
    <div class="head">
      <h3>{new Date(selected + "T12:00").toLocaleDateString([], { weekday: "long", day: "numeric", month: "long", year: "numeric" })}</h3>
      {#if report && (report.text || lit)}
        <button class="ghost write" disabled={summarizing} onclick={summarize}>
          {summarizing ? "Writing…" : report.text ? "Write again" : "Write the day"}
        </button>
      {/if}
    </div>

    {#if report?.text}
      <div class="report"><Markdown text={report.text} rich /></div>
    {:else if report && selected === today}
      <p class="quiet">Today's entry is written tonight, after midnight.</p>
    {:else if report && lit}
      <Quiet text="Nothing yet" />
    {:else if report}
      <Quiet text="Nothing happened" />
    {/if}

    {#if report && (report.text || lit)}
      <button class="ghost more" onclick={loadDetail} aria-expanded={showDetail}>{showDetail ? "Hide what happened" : "Show what happened"}</button>
    {/if}

    {#if !showDetail}
      <!-- The threads, sessions and commits behind the entry, only when asked for. -->
    {:else if !detail}
      {#if detailLoading}<Quiet kind="loading" />{/if}
    {:else if empty(detail)}
      <Quiet text="Nothing happened" />
    {:else}
      {#if detail.threads.length}
        <h4>Threads</h4>
        {#each detail.threads as t (t.id)}
          {@const live = hub.threads.find((x) => x.id === t.id)}
          <div class="thread">
            <div class="line">
              {#if live}
                <span class="dot ring {live.status}" class:unread={unread(live)}></span>
                <button class="title link" onclick={() => hub.open(t.id)}>{t.title}</button>
              {:else}
                <span class="title">{t.title}</span>
              {/if}
              <span class="where"><Ref kind="project" label={folderName(t.cwd)} title={t.cwd} onopen={() => hub.openProject(t.cwd)} /></span>
              <span class="count">{plural(t.messages.length, "message")} · {plural(t.turns, "turn")}{t.tokens ? ` · ${tokens(t.tokens)} tokens` : ""}</span>
            </div>
            {#each t.messages.slice(0, 3) as m, i (i)}<div class="said">{m}</div>{/each}
            {#if t.messages.length > 3}<div class="said more">and {t.messages.length - 3} more</div>{/if}
          </div>
        {/each}
      {/if}

      {#if detail.sessions.length}
        <h4>Claude Code</h4>
        {#each detail.sessions as s (s.id)}
          <div class="thread">
            <div class="line">
              <span class="title">{s.title ?? s.prompts[0]}</span>
              {#if s.cwd}<span class="where"><Ref kind="project" label={folderName(s.cwd)} title={s.cwd} onopen={() => hub.openProject(s.cwd!)} /></span>{/if}
              <span class="count">{plural(s.prompts.length, "prompt")}{s.tokens ? ` · ${tokens(s.tokens)} tokens` : ""}</span>
            </div>
            {#each s.prompts.slice(0, 3) as p, i (i)}<div class="said">{p}</div>{/each}
            {#if s.prompts.length > 3}<div class="said more">and {s.prompts.length - 3} more</div>{/if}
          </div>
        {/each}
      {/if}

      {#if detail.github.length || detail.pulls.length}
        <h4>GitHub</h4>
        {#each detail.pulls as p (p.url)}
          <div class="commit"><span class="subject"><Ref kind="pr" tone="open" label={p.title} title={p.url} onopen={() => open(p.url)} /></span><span class="when">{p.repo}</span></div>
        {/each}
        {#each detail.github as r (r.repo)}
          <div class="commit"><span class="subject"><Ref kind="repo" label={r.repo} /></span><span class="when">{plural(r.commits, "commit")}</span></div>
        {/each}
      {/if}

      {#if commitsByRepo.length}
        <h4>Commits on this computer</h4>
        {#each commitsByRepo as [repo, list] (repo)}
          <div class="repo"><Ref kind="repo" label={folderName(repo)} title={repo} onopen={() => openRepo(repo)} /></div>
          {#each list as c (c.hash)}
            <div class="commit"><span class="hash mono">{shortVersion(c.hash)}</span><span class="subject">{c.subject}</span><span class="when">{clock(c.ts)}</span></div>
          {/each}
        {/each}
      {/if}
    {/if}
{/if}

<style>
  .dot { display: block; flex: none; }
  /* A thread's state ring in the day's list. */
  .dot.ring { width: 8px; height: 8px; border-radius: 50%; display: inline-block; align-self: center; margin-right: -4px; }
  .head { display: flex; align-items: center; gap: 12px; }
  h3 { flex: 1; color: var(--text); font-size: 13px; font-weight: 500; margin: 0; }
  h4 { color: var(--muted); font-size: 12px; font-weight: 500; margin: 16px 0 6px; }
  .write { font-size: 13px; color: var(--text-2); background: var(--surface-hover); border-radius: var(--r-md); padding: 4px 10px; }
  .write:hover:not(:disabled) { color: var(--text); }
  .write:disabled { color: var(--muted); cursor: default; }
  .quiet { color: var(--muted); margin: 10px 0 0; font-size: 12px; }
  /* Prose keeps a readable width however wide the card is. */
  .report { max-width: var(--prose); color: var(--text-2); margin: 12px 0 0; padding: 10px 14px; background: var(--bg); border-radius: var(--r-md); }
  .more { margin-top: 10px; }
  .summary { color: var(--text-2); margin: 12px 0 0; padding: 10px 12px; background: var(--bg); border-radius: var(--r-md); white-space: pre-wrap; }

  .thread { padding: 6px 0; }
  .thread + .thread { border-top: 1px solid var(--line); }
  .line { display: flex; align-items: baseline; gap: 10px; }
  .title { color: var(--text); text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .link { text-align: left; }
  .link:hover { color: var(--accent); }
  .where, .count, .when { color: var(--muted); font-size: 12px; flex: none; }
  .count { margin-left: auto; }
  .said { color: var(--text-3); font-size: 13px; padding-left: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .said.more { color: var(--muted); }
  .repo { color: var(--text-3); font-size: 13px; margin: 6px 0 2px; }
  .commit { display: flex; gap: 10px; align-items: baseline; padding: 2px 0 2px 12px; }
  .hash { color: var(--muted); font-size: 12px; flex: none; }
  .subject { flex: 1; min-width: 0; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
