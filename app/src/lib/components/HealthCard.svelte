<!-- The weekly health check as a picture: one square per check across every project, green when fine, amber to look at, red broken, worst projects first; under it a row for each project that has something wrong, and one line with the count and when the check ran. Click a square or a row to see that project's checks. Hub looks without running anything; "Build and test" starts a thread that does, in that project, and Check now is in the card's menu.
     Used by CardBody.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // The weekly health check as a picture: one square per check across every project, green when
  // fine, amber to look at, red broken, worst projects first; under it a row for each project
  // that has something wrong, and one line with the count and when the check ran. Click a square
  // or a row to see that project's checks. Hub looks without running anything; "Build and test"
  // starts a thread that does, in that project, and Check now is in the card's menu.
  import { api } from "../api";
  import type { HealthReport } from "../bindings/HealthReport";
  import type { ProjectHealth } from "../bindings/ProjectHealth";
  import { ago } from "../format";
  import { onAction } from "../overview.svelte";
  import { recall, remember } from "../lastSeen";
  import { quiet } from "../quiet";
  import { hub } from "../state.svelte";
  import HowCounted from "./HowCounted.svelte";
  import Ref from "./Ref.svelte";

  const DEEP = `Health check of this project. Do not change any files. Find out how the project is built and tested (README, CLAUDE.md, manifests, CI config), then build it and run its tests. Also list dependencies with known vulnerabilities or far behind their latest version, using the package manager's own audit or outdated command where there is one. Report back briefly: does it build, are the tests green, which dependencies need attention, and anything else that looks broken. Lead with a one-line verdict.`;

  let report = $state<HealthReport | null>(recall<HealthReport>("health"));
  let checking = $state(false);
  let open = $state<string | null>(null);

  quiet(api.checkup(), "health checkup").then((r) => {
    if (!r) return;
    report = r;
    remember("health", r);
  });

  async function run() {
    if (checking) return;
    checking = true;
    try {
      report = await api.runCheckup();
      remember("health", report);
    } catch (e) {
      hub.fail(e);
    } finally {
      checking = false;
    }
  }
  onAction("health", "check", run);

  /** The checks worth a line: the ones that are not fine. */
  const issues = (p: ProjectHealth) => p.checks.filter((c) => c.health === "warn" || c.health === "bad");
  const projects = $derived(report?.projects ?? []);
  const counts = $derived({ bad: projects.filter((p) => p.health === "bad").length, warn: projects.filter((p) => p.health === "warn").length, checks: projects.reduce((n, p) => n + p.checks.length, 0) });
  const wrong = $derived(projects.filter((p) => p.health !== "ok"));
  const toggle = (path: string) => (open = open === path ? null : path);
</script>

<HowCounted card="health">
  <p>Weekly look at CI, unpushed work and old branches.</p>
</HowCounted>

{#if !report}
  <Quiet kind="loading" />
{:else if !report.checked_at}
  <p class="quiet">No check yet.</p>
{:else if !projects.length}
  <Quiet text="No projects" />
{:else}
  <div class="grid">
    {#each projects as p (p.path)}
      {#each p.checks as c, i (p.path + i)}
        <button class="sq {c.health}" class:open={open === p.path} data-tip="{p.name} · {c.label}: {c.detail}" aria-label="{p.name} · {c.label}: {c.detail}" onclick={() => toggle(p.path)}></button>
      {/each}
    {/each}
  </div>
  {#each wrong as p (p.path)}
    <div class="project">
      <div class="row">
        <span class="state {p.health}"></span>
        <span class="name"><Ref kind="project" project={{ path: p.path }} label={p.name} title="Open {p.path}" onopen={() => hub.openProject(p.path)} /></span>
        <button class="what" onclick={() => toggle(p.path)} data-tip="Show the checks">{issues(p).map((c) => c.detail).join(" · ")}</button>
      </div>
    </div>
  {/each}
  {#each projects.filter((p) => open === p.path) as p (p.path)}
    <div class="checks">
      <div class="check head"><span class="label">{p.name}</span></div>
      {#each p.checks as c}
        <div class="check">
          <span class="state {c.health}"></span>
          <span class="label">{c.label}</span>
          {#if c.url}<a href={c.url} target="_blank" rel="noreferrer">{c.detail}</a>{:else}<span>{c.detail}</span>{/if}
        </div>
      {/each}
      <button class="action" onclick={() => hub.newThread(p.path, DEEP)}>Build and test in a thread</button>
    </div>
  {/each}
  <p class="caption">
    {#if checking}Checking…
    {:else}
      {#if counts.bad}<em class="minus">{counts.bad} broken</em>{:else}<em>All fine</em>{/if}{#if counts.warn}{" · "}{counts.warn} to look at{/if}{" · "}{counts.checks} checks on {projects.length} {projects.length === 1 ? "project" : "projects"} · {ago(report.checked_at)}
    {/if}
  </p>
{/if}

<style>
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(14px, 1fr)); gap: 3px; }
  .sq { aspect-ratio: 1; border-radius: 2px; background: var(--surface-hover); padding: 0; min-width: 0; }
  .sq.ok { background: color-mix(in srgb, var(--green) 32%, var(--surface-hover)); }
  .sq.warn { background: color-mix(in srgb, var(--amber) 60%, var(--surface-hover)); }
  .sq.bad { background: var(--red); }
  .sq:hover { outline: 1px solid var(--text-3); outline-offset: 1px; }
  .sq.open { outline: 1.5px solid var(--text); outline-offset: 1px; }
  .project { margin-top: 8px; }
  .project + .project { margin-top: 0; }
  .row { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 0; font-size: 13px; }
  .row .what { text-align: left; flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-3); }
  .row .what:hover { color: var(--text-2); }
  .name { flex: none; max-width: 40%; display: flex; min-width: 0; overflow: hidden; color: var(--text-2); font-weight: 500; }
  .state { width: 7px; height: 7px; border-radius: 50%; flex: none; border: 1.5px solid var(--muted); }
  .state.ok { background: var(--green); border-color: var(--green); }
  .state.warn { background: var(--amber); border-color: var(--amber); }
  .state.bad { background: var(--red); border-color: var(--red); }
  .checks { margin-top: 8px; padding: 8px 10px; background: var(--bg); border-radius: var(--r-md); display: flex; flex-direction: column; gap: 4px; align-items: flex-start; }
  .check { display: flex; align-items: center; gap: 10px; font-size: 13px; color: var(--text-3); }
  .check.head .label { color: var(--text-2); font-weight: 500; width: auto; }
  .check .label { width: 96px; color: var(--muted); }
  .check a { color: var(--accent); }
  .action { font-size: 12px; color: var(--text-2); background: var(--surface-hover); border-radius: var(--r-md); padding: 3px 8px; margin-top: 4px; }
  .action:hover { color: var(--text); }
</style>
