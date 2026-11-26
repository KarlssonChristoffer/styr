<!-- The project page's recipes: the user's saved jobs, each one click from running here. Writing and changing them happens on the Recipes page.
     Used by ProjectView.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import Quiet from "./Quiet.svelte";
  // The project page's recipes: the user's saved jobs, each one click from running here. Writing
  // and changing them happens on the Recipes page.
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { Recipe } from "../bindings/Recipe";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let { path }: { path: string } = $props();
  let recipes = $state<Recipe[]>([]);
  let busy = $state<string | null>(null);

  const same = (a: string, b: string) => a.replace(/[\\/]+$/, "").toLowerCase() === b.replace(/[\\/]+$/, "").toLowerCase();
  // Recipes for any project, and the ones tied to this one.
  const here = $derived(recipes.filter((r) => !r.suggested && (!r.folder || same(r.folder, path))));

  onMount(() => {
    quiet(api.recipes().then((d) => (recipes = d.recipes)), "recipes");
  });

  async function run(r: Recipe) {
    busy = r.id;
    try {
      const { thread } = await api.runRecipe(r.id, path);
      hub.open(thread);
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = null;
    }
  }
</script>

<section>
  <div class="card-head">
    <h2>Recipes</h2>
    <button class="more" onclick={() => (hub.view = "recipes")}>Manage</button>
  </div>
  {#if here.length}
    <div class="chips">
      {#each here as r (r.id)}
        <button class="chip" title={r.about ? `${r.about} Runs here and stops at a pull request.` : "Runs here and stops at a pull request."} disabled={busy === r.id} onclick={() => run(r)}><Icon name="play" size={10} />{r.name}</button>
      {/each}
    </div>
  {:else}
    <Quiet text="No recipes" />
  {/if}
</section>

<style>
  section { background: var(--surface); border-radius: var(--r-lg); padding: 16px 18px 12px; min-width: 0; }
  .card-head { display: flex; align-items: center; gap: 10px; margin-bottom: 8px; }
  h2 { color: var(--text); font-size: 14px; font-weight: 500; margin: 0; }
  .more { margin-left: auto; color: var(--muted); font-size: 12px; }
  .more:hover { color: var(--text); }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; padding-bottom: 4px; }
  .chip { display: inline-flex; align-items: center; gap: 4px; min-height: 24px; padding: 2px 10px; border-radius: 999px; border: 1px solid var(--line-strong); color: var(--text-2); font-size: 13px; text-align: left; }
  .chip :global(svg) { color: var(--muted); flex: none; }
  .chip:hover:not(:disabled) { color: var(--text); border-color: var(--accent); }
  .chip:disabled { opacity: 0.5; cursor: default; }
</style>
