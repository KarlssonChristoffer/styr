<!-- Today: the day in numbers, what it was spent on, what went into the repositories and how the devlog tells it. Read from the same places the Overview reads.
     Used by Scenes.svelte. -->
<script lang="ts">
  import Quiet from "../Quiet.svelte";
  // Today: the day in numbers, what it was spent on, what went into the repositories and how the
  // devlog tells it. Read from the same places the Overview reads.
  import { api } from "../../api";
  import type { ActivityDetail } from "../../bindings/ActivityDetail";
  import type { Stats } from "../../bindings/Stats";
  import { folderName } from "../../format";
  import { dateKey } from "../../overview.svelte";
  import { hub } from "../../state.svelte";
  import { openUrl } from "../../native";
  import Markdown from "../Markdown.svelte";

  const tz = -new Date().getTimezoneOffset();
  const date = dateKey(new Date());
  let day = $state<ActivityDetail | null>(null);
  let stats = $state<Stats | null>(null);
  $effect(() => {
    api.activityDay(date, tz).then((d) => (day = d), () => {});
    api.stats(1, tz).then((s) => (stats = s), () => {});
  });
  const hhmm = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
  const startOfDay = new Date().setHours(0, 0, 0, 0);
  const moved = $derived(hub.active.filter((t) => t.updated_at >= startOfDay).length);
  const added = $derived(day?.commits.reduce((n, c) => n + c.added, 0) ?? 0);
  const removed = $derived(day?.commits.reduce((n, c) => n + c.removed, 0) ?? 0);
  const tokens = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)} M` : n >= 1e3 ? `${Math.round(n / 1e3)} k` : String(n));
  const models = $derived(stats?.models.slice(0, 4) ?? []);
  const projects = $derived(stats?.projects.slice(0, 4) ?? []);
  const mostM = $derived(Math.max(1, ...models.map((m) => m.tokens)));
  const mostP = $derived(Math.max(1, ...projects.map((p) => p.tokens)));
</script>

<section class="s12 c"><div class="lbl">Today</div>
  <div class="blk p"><div class="stats">
    <div><div class="fig">{day ? day.commits.length : "·"}</div><div class="cap">commits</div></div>
    <div><div class="fig">{day ? day.pulls.length : "·"}</div><div class="cap">PRs opened</div></div>
    <div><div class="fig"><span class="plus">+{added.toLocaleString("sv-SE")}</span> <span class="minus">−{removed.toLocaleString("sv-SE")}</span></div><div class="cap">lines</div></div>
    <div><div class="fig">{moved}</div><div class="cap">threads moved</div></div>
    <div><div class="fig">{stats ? tokens(stats.tokens) : "·"}</div><div class="cap">tokens</div></div>
  </div></div></section>
<section class="s6 c"><div class="lbl">Models</div>
  <div class="blk p">
    {#if !stats}<Quiet kind="loading" />
    {:else if !models.length}<Quiet text="No answers yet today" />
    {:else}<div class="hb">{#each models as m (m.model)}<span>{m.model}</span><span class="bar"><i style:width="{(m.tokens / mostM) * 100}%"></i></span><span class="v">{tokens(m.tokens)}</span>{/each}</div>{/if}
  </div></section>
<section class="s6"><div class="lbl">Projects</div>
  <div class="blk p">
    {#if !stats}<Quiet kind="loading" />
    {:else if !projects.length}<Quiet text="Nothing spent yet today" />
    {:else}<div class="hb">{#each projects as p (p.name)}<span>{p.name}</span><span class="bar"><i style:width="{(p.tokens / mostP) * 100}%"></i></span><span class="v">{tokens(p.tokens)}</span>{/each}</div>{/if}
  </div></section>
<section class="s6"><div class="lbl">Commits <span class="n">{day?.commits.length ?? 0}</span></div>
  <div class="blk">
    {#if day && !day.commits.length}<Quiet text="No commits yet today" />{/if}
    {#each (day?.commits ?? []).slice(0, 6) as c (c.hash)}
      <div class="plain"><span class="tm">{hhmm(c.ts)}</span> {c.subject} <span class="m">· {folderName(c.repo)}</span></div>
    {/each}
  </div></section>
<section class="s6"><div class="lbl">Pull requests opened <span class="n">{day?.pulls.length ?? 0}</span></div>
  <div class="blk">
    {#if day && !day.pulls.length}<Quiet text="None today" />{/if}
    {#each day?.pulls ?? [] as p (p.url)}
      <button class="line" onclick={() => openUrl(p.url).catch((e) => hub.fail(e))}><span class="t">{p.title}</span><span class="m">{p.repo}</span></button>
    {/each}
  </div></section>
{#if day?.summary}
  <section class="s12"><div class="lbl">Devlog</div><div class="blk p"><div class="log"><Markdown text={day.summary} /></div></div></section>
{/if}

<style>
  .stats { display: flex; gap: 28px; flex-wrap: wrap; padding: 2px 0; }
  .fig { color: var(--text); font-size: 16px; font-weight: 500; letter-spacing: -0.01em; line-height: 1; font-variant-numeric: tabular-nums; }
  .cap { color: var(--muted); font-size: 12px; margin-top: 4px; }
  .plus { color: var(--green); }
  .minus { color: var(--red); }
  .plain { padding: 4px 0; font-size: 13px; color: var(--text-2); line-height: 1.4; }
  .tm { color: var(--muted); font-size: 12px; margin-right: 6px; font-variant-numeric: tabular-nums; }
  .m { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .line { display: flex; align-items: center; gap: 10px; width: 100%; min-height: 28px; padding: 0; text-align: left; font-size: 13px; color: var(--text-2); }
  .line:hover .t { color: var(--text); }
  .t { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .log { font-size: 13px; color: var(--text-2); max-height: 220px; overflow: hidden; }
</style>
