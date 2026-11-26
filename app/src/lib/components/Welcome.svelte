<!-- Welcome: a short guide to where things are and what to do after a project is created. Plain text and a few jumps; nothing here changes anything.
     Used by App.svelte. -->
<script lang="ts">
  // Welcome: a short guide to where things are and what to do after a project is created. Plain
  // text and a few jumps; nothing here changes anything.
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  const steps: { title: string; go?: { label: string; run: () => void } }[] = [
    { title: "Lägg till ett projekt", go: { label: "Nytt projekt", run: () => (hub.newProjectOpen = true) } },
    { title: "Starta en tråd" },
    { title: "Låt den jobba", go: { label: "Home", run: () => hub.goHome() } },
    { title: "Svara Customs", go: { label: "Customs", run: () => hub.openSettings("customs") } },
    { title: "Testa, sen merga" },
  ];
  const places: { label: string; view: "overview" | "machines" | "library" | "problems" }[] = [
    { label: "Overview", view: "overview" },
    { label: "Machines", view: "machines" },
    { label: "Library", view: "library" },
    { label: "Problems", view: "problems" },
  ];
  function open(view: (typeof places)[number]["view"]) {
    hub.view = view;
    hub.selected = null;
  }
</script>

<div class="welcome">
  <div class="page-fill">
    <h1>Välkommen till Hub</h1>

    <h2>När du har skapat ett projekt</h2>
    <ol class="steps">
      {#each steps as s, i (s.title)}
        <li>
          <span class="n">{i + 1}</span>
          <div class="body">
            <div class="t">{s.title}</div>
            {#if s.go}<button class="link" onclick={s.go.run}>{s.go.label}</button>{/if}
          </div>
        </li>
      {/each}
    </ol>

    <h2>Var saker finns</h2>
    <div class="places">
      {#each places as p (p.view)}
        <button class="place" onclick={() => open(p.view)}>
          <span class="pt">{p.label}</span>
        </button>
      {/each}
    </div>

    <p class="foot"><Icon name="search" size={12} /> Ctrl K hittar och gör allt annat.</p>
  </div>
</div>

<style>
  .welcome { flex: 1; overflow-y: auto; min-width: 0; }
  .page-fill { max-width: 640px; }
  h1 { color: var(--text); font-size: var(--title); font-weight: 500; letter-spacing: -0.01em; margin: 0 0 14px; }
  h2 { color: var(--muted); font-size: 12px; font-weight: 500; margin: 20px 0 8px; }
  .steps { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  li { display: flex; gap: 10px; padding: 10px 12px; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); }
  .n { flex: none; width: 20px; height: 20px; display: grid; place-items: center; border-radius: 999px; background: var(--surface-2); color: var(--muted); font-size: 12px; }
  .t { color: var(--text); font-size: 14px; }
  .link { color: var(--accent); font-size: 13px; padding: 0; margin-top: 4px; }
  .link:hover { text-decoration: underline; }
  .places { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 6px; }
  .place { display: flex; flex-direction: column; gap: 2px; text-align: left; padding: 10px 12px; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); }
  .place:hover { background: var(--surface-hover); }
  .pt { color: var(--text); font-size: 14px; }
  .foot { display: flex; align-items: center; gap: 6px; color: var(--muted); font-size: 12px; margin: 16px 0 0; }
</style>
