<!-- What changed while you were away: threads that finished or failed since you left, the commits and pull requests that landed, what Hub did on its own, and what the health check found. Threads waiting on you have their own card on Home, so this only counts them.
     Used by Home.svelte. -->
<script lang="ts">
  // What changed while you were away: threads that finished or failed since you left, the commits
  // and pull requests that landed, what Hub did on its own, and what the health check found.
  // Threads waiting on you have their own card on Home, so this only counts them.
  import { api } from "../api";
  import { away } from "../away.svelte";
  import type { AwayReport } from "../bindings/AwayReport";
  import { menu } from "../menu.svelte";
  import { openRepo } from "../refs";
  import { threadMenu } from "../menus.svelte";
  import { hub, unread } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import Ref from "./Ref.svelte";
  import ThreadMeta from "./ThreadMeta.svelte";

  const since = $derived(away.since ?? 0);
  const changed = $derived(hub.active.filter((t) => t.updated_at > since && t.status !== "running" && t.status !== "waiting_on_you"));
  const failed = $derived(changed.filter((t) => t.status === "error"));
  const finished = $derived(changed.filter((t) => t.status === "idle"));
  const waiting = $derived(hub.waiting.filter((t) => t.updated_at > since).length);

  let report = $state<AwayReport | null>(null);
  $effect(() => {
    const from = away.since;
    report = null;
    if (from) api.away(from, -new Date().getTimezoneOffset()).then((r) => away.since === from && (report = r), () => {});
  });

  const commits = $derived(report?.repos.reduce((n, r) => n + r.commits, 0) ?? 0);
  /** What Hub did without asking, in one line. */
  const itself = $derived.by(() => {
    if (!report) return "";
    const n = (k: number, one: string, many: string) => (k ? `${k} ${k === 1 ? one : many}` : "");
    return [
      n(report.permissions, "permission question answered", "permission questions answered"),
      n(report.resumed, "thread carried on", "threads carried on"),
      n(report.reviews, "second opinion", "second opinions"),
      n(report.side_jobs, "side job finished", "side jobs finished"),
    ].filter(Boolean).join(", ");
  });
  const something = $derived(changed.length || waiting || commits || report?.pulls.length || report?.health.length);

  function gone(ms: number) {
    const min = Math.round((Date.now() - ms) / 60000);
    return min < 90 ? `${min} min` : min < 36 * 60 ? `${Math.round(min / 60)} h` : `${Math.round(min / 1440)} days`;
  }
</script>

{#if away.since && something}
  <section class="away">
    <h2>
      While you were away <span class="n">{gone(away.since)}</span>
      <button class="close" data-tip="Got it" onclick={() => away.dismiss()}><Icon name="x" size={13} /></button>
    </h2>
    <p class="line">
      {[
        finished.length && `${finished.length} finished`,
        failed.length && `${failed.length} failed`,
        waiting && `${waiting} started waiting on you`,
        commits && `${commits} ${commits === 1 ? "commit" : "commits"} in ${report?.repos.length} ${report?.repos.length === 1 ? "repo" : "repos"}`,
        report?.pulls.length && `${report.pulls.length} pull ${report.pulls.length === 1 ? "request" : "requests"} opened`,
      ].filter(Boolean).join(", ")}.
    </p>
    {#each [...failed, ...finished].slice(0, 6) as t (t.id)}
      <button class="item" onclick={() => hub.open(t.id)} use:menu={() => threadMenu(t)}>
        <span class="dot {t.status}" class:unread={unread(t)}></span><span class="t">{t.title}</span><span class="m"><ThreadMeta {t} /></span>
      </button>
    {/each}
    {#each report?.health ?? [] as p (p.path)}
      <button class="item" onclick={() => (hub.view = "overview")} data-tip="Open the health check">
        <span class="state {p.health}"></span>
        <span class="t"><Ref kind="project" project={{ path: p.path }} label={p.name} title={p.path} /></span>
        <span class="m">{p.checks.filter((c) => c.health === "warn" || c.health === "bad").map((c) => c.detail).join(" · ")}</span>
      </button>
    {/each}
    {#each report?.repos.slice(0, 4) ?? [] as r (r.path)}
      <button class="item" onclick={() => openRepo(r.path)} data-tip={r.subjects.join("\n")}>
        <span class="t"><Ref kind="repo" label={r.name} title={r.path} /><span class="sub">{r.subjects[0]}</span></span>
        <span class="m">{r.commits} {r.commits === 1 ? "commit" : "commits"}</span>
      </button>
    {/each}
    {#each report?.pulls.slice(0, 4) ?? [] as p (p.url)}
      <a class="item" href={p.url} target="_blank" rel="noreferrer">
        <span class="t"><Ref kind="pr" tone="open" label={p.title} /></span>
        <span class="m">{p.repo}</span>
      </a>
    {/each}
    {#if itself}<p class="itself">Hub on its own: {itself}.</p>{/if}
  </section>
{/if}

<style>
  .away { background: var(--surface); border-radius: var(--r-lg); padding: 14px 14px 8px; margin-bottom: 12px; border: 1px solid var(--line-strong); }
  h2 { display: flex; align-items: center; gap: 8px; color: var(--text); font-size: 14px; font-weight: 500; margin: 0 0 4px; }
  .n { color: var(--muted); font-weight: 400; }
  .close { margin-left: auto; color: var(--muted); padding: 2px; border-radius: var(--r-sm); display: grid; }
  .close:hover { color: var(--text); background: var(--surface-hover); }
  .line { color: var(--text-3); margin: 0 0 6px; font-size: 13px; }
  .item { display: flex; align-items: center; gap: 10px; padding: 8px 8px; margin: 0 -8px; border-radius: var(--r-md); text-align: left; width: calc(100% + 16px); color: inherit; text-decoration: none; }
  .item:hover { background: var(--surface-hover); }
  .t { flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); }
  .sub { min-width: 0; overflow: hidden; text-overflow: ellipsis; color: var(--text-3); font-size: 13px; }
  .m { color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 45%; }
  .state { width: 8px; height: 8px; border-radius: 50%; flex: none; }
  .state.warn { background: var(--amber); }
  .state.bad { background: var(--red); }
  .itself { color: var(--muted); font-size: 12px; margin: 4px 0 6px; }
</style>
