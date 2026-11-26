<!-- Projects: where each of your projects stands, one row each: what runs in it, what waits on you, what failed, and how long ago anything moved. The ones used last show unless you choose (Customize menu > Choose projects…): then exactly those, and a scratch folder never counts. A click opens the project; the plus starts a thread in it.
     Used by CardBody.svelte. -->
<script lang="ts">
  // Projects: where each of your projects stands, one row each: what runs in it, what waits on you,
  // what failed, and how long ago anything moved. The ones used last show unless you choose
  // (Customize menu > Choose projects…): then exactly those, and a scratch folder never counts.
  // A click opens the project; the plus starts a thread in it.
  import { ago, byProject, isScratch } from "../format";
  import { projectKey } from "../looks.svelte";
  import { onAction } from "../overview.svelte";
  import { prefs, setPref } from "../prefs.svelte";
  import { hub } from "../state.svelte";
  import { projectCounts } from "../threadFlags";
  import HowCounted from "./HowCounted.svelte";
  import Icon from "./Icon.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";
  import Switch from "./Switch.svelte";

  const MANY = 8;
  const all = $derived(
    byProject(hub.everything, hub.projects)
      .filter((p) => !isScratch(p.cwd))
      .map((p) => {
        return {
          ...p,
          id: projectKey(p.cwd, p.host),
          ...projectCounts(p.threads),
        };
      }),
  );
  const picked = $derived(prefs.homeProjects);
  const chosen = $derived(picked ? all.filter((p) => picked.includes(p.id)) : all.slice(0, MANY));
  /** What needs you first, then what runs, then the rest by how lately it moved. */
  const rank = (p: { asks: number; failed: number; running: number }) => (p.asks || p.failed ? 0 : p.running ? 1 : 2);
  const shown = $derived([...chosen].sort((a, b) => rank(a) - rank(b) || b.at - a.at));

  let choosing = $state(false);
  onAction("projects", "pick", () => (choosing = !choosing));
  function toggle(id: string, on: boolean) {
    const now = new Set(picked ?? chosen.map((p) => p.id));
    if (on) now.add(id);
    else now.delete(id);
    setPref("homeProjects", [...now]);
  }
</script>

<HowCounted card="projects">
  <p>Threads running, asking you or failed, per project.</p>
</HowCounted>

{#if choosing}
  <div class="pick">
    <div class="head"><span>Show these projects</span>
      {#if picked}<button class="link" onclick={() => setPref("homeProjects", null)}>Show the latest {MANY}</button>{/if}
      <button class="link" onclick={() => (choosing = false)}>Done</button>
    </div>
    {#each all as p (p.id)}
      <div class="opt">
        <ProjectIcon path={p.cwd} host={p.host} name={p.name} size={14} />
        <span class="name">{p.name}{#if p.host}<span class="host"> · {p.host}</span>{/if}</span>
        <Switch on={picked ? picked.includes(p.id) : chosen.some((c) => c.id === p.id)} label="Show {p.name}" onchange={(on) => toggle(p.id, on)} />
      </div>
    {/each}
  </div>
{:else if !shown.length}
  <p class="quiet">{picked ? "None of the chosen projects exist any more." : "No projects yet. Start a thread in a folder below."}</p>
{:else}
  <div class="list">
    {#each shown as p (p.id)}
      <div class="line" class:needs={p.asks > 0 || p.failed > 0}>
        <button class="open" onclick={() => hub.openProject(p.cwd, p.host)} data-tip={p.host ? `${p.host}: ${p.cwd}` : p.cwd}>
          <ProjectIcon path={p.cwd} host={p.host} name={p.name} size={16} />
          <span class="name">{p.name}</span>
          <span class="grow"></span>
          {#if p.failed}<span class="s red">{p.failed} failed</span>{/if}
          {#if p.asks}<span class="s amber">{p.asks === 1 ? "1 needs you" : `${p.asks} need you`}</span>{/if}
          {#if p.running}<span class="s green">{p.running} running</span>{/if}
          {#if !p.failed && !p.asks && !p.running}<span class="s">{p.at ? `idle · ${ago(p.at)}` : "no threads"}</span>{/if}
        </button>
        <button class="plus" onclick={() => hub.startDraft(p.cwd, "", p.host)} data-tip="New thread in {p.name}" aria-label="New thread in {p.name}"><Icon name="plus" size={13} /></button>
      </div>
    {/each}
  </div>
  <p class="caption"><em>{shown.length}</em> {picked ? "chosen" : "latest"}{all.length > shown.length ? ` of ${all.length}` : ""}</p>
{/if}

<style>
  .list { display: flex; flex-direction: column; }
  .line { display: flex; align-items: center; gap: 6px; min-width: 0; border-bottom: 1px solid var(--line); }
  .line:last-child { border-bottom: 0; }
  .open { display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0; height: 36px; text-align: left; padding: 0; font-size: 13px; color: var(--text-2); }
  .open:hover .name { color: var(--text); }
  .name { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .grow { flex: 1; }
  .s { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .s.red { color: var(--red); }
  .s.amber { color: var(--amber); }
  .s.green { color: var(--green); }
  .plus { width: 24px; height: 24px; border-radius: var(--r-sm); display: grid; place-items: center; color: var(--muted); opacity: 0; flex: none; }
  .line:hover .plus, .plus:focus-visible { opacity: 1; }
  .plus:hover { color: var(--text); background: var(--surface-hover); }
  .pick .head { display: flex; align-items: center; gap: 12px; color: var(--text-2); font-size: 13px; margin-bottom: 6px; }
  .pick .head span { flex: 1; }
  .link { color: var(--accent); font-size: 12px; }
  .link:hover { text-decoration: underline; text-underline-offset: 2px; }
  .opt { display: flex; align-items: center; gap: 10px; height: 32px; font-size: 13px; color: var(--text-2); }
  .opt .name { flex: 1; min-width: 0; }
  .host { color: var(--muted); }
</style>
