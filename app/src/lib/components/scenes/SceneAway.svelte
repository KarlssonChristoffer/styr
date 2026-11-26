<!-- While you were away: what changed since you left, as one list in time order, what landed in your repositories, what Hub did on its own and what the health check found. Without a gap to look back on (you have not been away, or you closed it) it shows the last twelve hours.
     Used by Scenes.svelte. -->
<script lang="ts">
  import Quiet from "../Quiet.svelte";
  // While you were away: what changed since you left, as one list in time order, what landed in
  // your repositories, what Hub did on its own and what the health check found. Without a gap to
  // look back on (you have not been away, or you closed it) it shows the last twelve hours.
  import { api } from "../../api";
  import { away } from "../../away.svelte";
  import type { AwayReport } from "../../bindings/AwayReport";
  import type { Thread } from "../../bindings/Thread";
  import { folderName, threadProject } from "../../format";
  import { menu } from "../../menu.svelte";
  import { threadMenu } from "../../menus.svelte";
  import { openRepo } from "../../refs";
  import { hub } from "../../state.svelte";
  import { openUrl } from "../../native";
  import ToTest from "./ToTest.svelte";

  const hhmm = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
  const since = $derived(away.since ?? Date.now() - 12 * 3_600_000);
  const changed = $derived(
    hub.active
      .filter((t) => (t.returned_at ?? t.updated_at) > since && t.status !== "running")
      .sort((a, b) => (b.returned_at ?? b.updated_at) - (a.returned_at ?? a.updated_at)),
  );
  const word = (t: Thread) => (t.status === "error" ? "failed" : t.status === "waiting_on_you" ? "waits on you" : "finished");
  const colour = (t: Thread) => (t.status === "error" ? "var(--red)" : t.status === "waiting_on_you" ? "var(--amber)" : "var(--green)");

  let report = $state<AwayReport | null>(null);
  $effect(() => {
    const from = since;
    report = null;
    api.away(from, -new Date().getTimezoneOffset()).then((r) => since === from && (report = r), () => {});
  });
  const commits = $derived(report?.repos.reduce((n, r) => n + r.commits, 0) ?? 0);
  const most = $derived(Math.max(1, ...(report?.repos.map((r) => r.commits) ?? [])));
  const itself = $derived.by(() => {
    if (!report) return [] as string[];
    const n = (k: number, one: string, many: string) => (k ? `${k} ${k === 1 ? one : many}` : "");
    return [
      n(report.permissions, "permission question answered", "permission questions answered"),
      n(report.resumed, "thread carried on after a limit or restart", "threads carried on after a limit or restart"),
      n(report.reviews, "second opinion", "second opinions"),
      n(report.side_jobs, "side job finished", "side jobs finished"),
    ].filter(Boolean);
  });
  const gone = (ms: number) => {
    const min = Math.round((Date.now() - ms) / 60000);
    return min < 90 ? `${min} min` : min < 36 * 60 ? `${Math.round(min / 60)} h` : `${Math.round(min / 1440)} days`;
  };
</script>

<section class="s7 c"><div class="lbl">Since you left <span class="n">{gone(since)} ago</span>
    <span class="r">{#if away.since}<button class="link" onclick={() => away.dismiss()}>Got it</button>{:else}last 12 hours{/if}</span></div>
  <div class="blk">
    {#if !changed.length}<Quiet text="No thread finished, failed or asked for you" />{/if}
    <div class="feed">
      {#each changed.slice(0, 9) as t (t.id)}
        <span class="t">{hhmm(t.returned_at ?? t.updated_at)}</span>
        <span class="d"><i style:background={colour(t)}></i></span>
        <button class="x" onclick={() => hub.open(t.id)} use:menu={() => threadMenu(t)}>{t.title} <span>· {word(t)} · {folderName(threadProject(t))}</span></button>
      {/each}
    </div>
  </div></section>
<section class="s5 c"><div class="lbl">Hub did by itself <span class="n">{itself.length}</span></div>
  <div class="blk">
    {#if !report}<Quiet kind="loading" />
    {:else if !itself.length}<Quiet text="Nothing it needed to do on its own" />{/if}
    {#each itself as line (line)}<div class="plain">{line}</div>{/each}
  </div></section>
<section class="s7"><div class="lbl">Changed per project <span class="n">{commits} {commits === 1 ? "commit" : "commits"}</span></div>
  <div class="blk p">
    {#if !report}<Quiet kind="loading" />
    {:else if !report.repos.length}<Quiet text="No commits" />
    {:else}
      <div class="hb">
        {#each report.repos as r (r.path)}
          <button class="lk" onclick={() => openRepo(r.path)} data-tip={r.subjects.join("\n")}>{r.name}</button>
          <span class="bar"><i style:width="{(r.commits / most) * 100}%"></i></span><span class="v">{r.commits}</span>
        {/each}
      </div>
    {/if}
  </div></section>
<section class="s5"><div class="lbl">Pull requests opened <span class="n">{report?.pulls.length ?? 0}</span></div>
  <div class="blk">
    {#if report && !report.pulls.length}<Quiet kind="none" />{/if}
    {#each report?.pulls ?? [] as p (p.url)}
      <button class="line" onclick={() => openUrl(p.url).catch((e) => hub.fail(e))}><span class="t">{p.title}</span><span class="m">{p.repo}</span></button>
    {/each}
  </div></section>
{#if report?.health.length}
  <section class="s6"><div class="lbl">Health check found <span class="n">{report.health.length}</span></div>
    <div class="blk">
      {#each report.health as h (h.path)}<div class="plain"><b>{h.name}</b> <span class="m">{h.checks.filter((c) => c.health === "warn" || c.health === "bad").map((c) => c.label).join(", ")}</span></div>{/each}
    </div></section>
{/if}
<section class="s6"><div class="lbl">To test by hand</div><div class="blk"><ToTest /></div></section>

<style>
  .link { color: var(--accent); font-size: 12px; padding: 0; }
  .feed { display: grid; grid-template-columns: 40px 8px minmax(0, 1fr); column-gap: 8px; font-size: 13px; }
  .feed .t { color: var(--muted); font-size: 12px; white-space: nowrap; font-variant-numeric: tabular-nums; padding: 4px 0; }
  .feed .d { position: relative; }
  .feed .d i { position: absolute; left: 1px; top: 11px; width: 6px; height: 6px; border-radius: 50%; }
  .feed .x { padding: 4px 0; text-align: left; color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .feed .x span { color: var(--muted); }
  .feed .x:hover { color: var(--text); }
  .plain { padding: 4px 0; font-size: 13px; color: var(--text-2); line-height: 1.4; }
  .plain b { font-weight: 500; color: var(--text); }
  .lk { text-align: left; padding: 0; font-size: 13px; color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lk:hover { color: var(--text); }
  .line { display: flex; align-items: center; gap: 10px; width: 100%; min-height: 28px; padding: 0; text-align: left; font-size: 13px; color: var(--text-2); }
  .line:hover .t { color: var(--text); }
  .t { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .m { color: var(--muted); font-size: 12px; white-space: nowrap; }
</style>
