<!-- What is left to test by hand, per project: the "To test" lines of each project's docs/FOCUS.md, as the core counts them. A click opens the project, where they are listed.
     Used by scenes/SceneAway.svelte, scenes/SceneNeeds.svelte, scenes/SceneProjects.svelte. -->
<script lang="ts">
  import { quiet } from "../../quiet";
  import Quiet from "../Quiet.svelte";
  // What is left to test by hand, per project: the "To test" lines of each project's docs/FOCUS.md,
  // as the core counts them. A click opens the project, where they are listed.
  import { api } from "../../api";
  import type { FocusSummary } from "../../bindings/FocusSummary";
  import { byProject, isScratch } from "../../format";
  import { hub } from "../../state.svelte";
  import Ref from "../Ref.svelte";

  const projects = $derived(byProject(hub.everything, hub.projects).filter((p) => !p.host && !isScratch(p.cwd)).slice(0, 12));
  const key = $derived(projects.map((p) => p.cwd).join("\n"));
  let focus = $state<Record<string, FocusSummary>>({});
  $effect(() => {
    const paths = key ? key.split("\n") : [];
    if (!paths.length) return;
    let live = true;
    quiet(api.focusSummaries(paths).then((f) => live && (focus = f)), "focus summaries");
    return () => (live = false);
  });
  const rows = $derived(projects.map((p) => ({ p, n: focus[p.cwd]?.to_test ?? 0 })).filter((r) => r.n > 0));
</script>

{#if !rows.length}
  <Quiet text="Nothing to test by hand" />
{:else}
  {#each rows as { p, n } (p.key)}
    <button class="line" onclick={() => hub.openProject(p.cwd, p.host)}>
      <Ref kind="project" project={{ path: p.cwd, host: p.host }} label={p.name} />
      <span class="grow"></span>
      <span class="m">{n} to test</span>
    </button>
  {/each}
{/if}

<style>
  .line { display: flex; align-items: center; gap: 10px; width: 100%; min-height: 28px; text-align: left; padding: 0; font-size: 14px; color: var(--text-2); }
  .line:hover .m { color: var(--text); }
  .grow { flex: 1; }
  .m { color: var(--muted); font-size: 12px; white-space: nowrap; }
</style>
