<!-- Projects: where each one stands, the ones used last first, and a way back into the thread that finished last. Shown when nothing runs and nothing waits on you.
     Used by Scenes.svelte. -->
<script lang="ts">
  import Quiet from "../Quiet.svelte";
  // Projects: where each one stands, the ones used last first, and a way back into the thread
  // that finished last. Shown when nothing runs and nothing waits on you.
  import { ago, byProject, isScratch } from "../../format";
  import { projectKey } from "../../looks.svelte";
  import { prefs } from "../../prefs.svelte";
  import { hub } from "../../state.svelte";
  import { projectCounts } from "../../threadFlags";
  import Ref from "../Ref.svelte";
  import ThreadRows from "./ThreadRows.svelte";
  import ToTest from "./ToTest.svelte";

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
  const rank = (p: { asks: number; failed: number; running: number }) => (p.asks || p.failed ? 0 : p.running ? 1 : 2);
  const shown = $derived([...chosen].sort((a, b) => rank(a) - rank(b) || b.at - a.at));

  const finished = $derived(
    hub.active
      .filter((t) => t.status === "idle" && !t.settled_at && t.returned_at != null)
      .sort((a, b) => (b.returned_at ?? 0) - (a.returned_at ?? 0))
      .slice(0, 4),
  );
</script>

<section class="s12 c"><div class="lbl">Pick up where you left off <span class="r">last used first</span></div>
  <div class="blk">
    {#if !shown.length}<Quiet text="No projects" />{/if}
    {#each shown as p (p.id)}
      <button class="pj" onclick={() => hub.openProject(p.cwd, p.host)}>
        <Ref kind="project" project={{ path: p.cwd, host: p.host }} label={p.name} />
        <span class="st">
          {#if p.asks}<span class="am">{p.asks} {p.asks === 1 ? "needs" : "need"} you</span>{/if}
          {#if p.failed}<span class="rd">{p.failed} failed</span>{/if}
          {#if p.running}<span class="gr">{p.running} running</span>{/if}
          {#if !p.asks && !p.failed && !p.running}{p.threads.length} {p.threads.length === 1 ? "thread" : "threads"}{/if}
        </span>
        <span class="rt">{p.at ? ago(p.at) : ""}</span>
      </button>
    {/each}
  </div></section>
<section class="s6 c"><div class="lbl">Last finished</div>
  <div class="blk"><ThreadRows threads={finished} empty="Nothing has finished yet." /></div></section>
<section class="s6"><div class="lbl">To test by hand</div><div class="blk"><ToTest /></div></section>

<style>
  .pj { display: grid; grid-template-columns: minmax(120px, 0.9fr) minmax(0, 1.6fr) 46px; align-items: center; column-gap: 12px; width: 100%; min-height: 28px; padding: 3px 0; text-align: left; font-size: 14px; color: var(--text); }
  .pj:hover { color: var(--accent); }
  .st { display: flex; gap: 10px; color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; }
  .st .am { color: var(--amber); }
  .st .rd { color: var(--red); }
  .st .gr { color: var(--green); }
  .rt { color: var(--muted); font-size: 12px; text-align: right; font-variant-numeric: tabular-nums; }
</style>
