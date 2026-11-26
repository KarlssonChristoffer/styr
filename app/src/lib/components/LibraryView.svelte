<!-- The library: one folder of linked notes about everything you run and build, mostly for the agents. You ask it things, and the thread that answers writes back what it learned. Hub keeps a note for each repo, technology and machine it knows, with the facts it can vouch for. No tree: you arrive at a note from a link, from Ctrl O or from the front page.
     Used by App.svelte. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { hub } from "../state.svelte";
  import Page from "./Page.svelte";
  import DocsView from "./DocsView.svelte";
  import Icon from "./Icon.svelte";

  // The library: one folder of linked notes about everything you run and build, mostly for the
  // agents. You ask it things, and the thread that answers writes back what it learned. Hub keeps
  // a note for each repo, technology and machine it knows, with the facts it can vouch for. No
  // tree: you arrive at a note from a link, from Ctrl O or from the front page.
  let root = $state<string | null>(null);
  let question = $state("");
  // Reading the library again when you come back picks up what agents wrote meanwhile.
  let version = $state(0);

  onMount(() => {
    api.library().then((l) => (root = l.root)).catch((e) => hub.fail(e));
    const again = () => document.visibilityState === "visible" && version++;
    document.addEventListener("visibilitychange", again);
    return () => document.removeEventListener("visibilitychange", again);
  });

  function ask(e: Event) {
    e.preventDefault();
    const q = question.trim();
    if (!q) return;
    question = "";
    hub.askLibrary(q);
  }
</script>

<Page title="Library" width="full" fill>
  {#snippet head()}
    <form class="ask" onsubmit={ask}>
      <Icon name="brain" size={14} />
      <input bind:value={question} placeholder="Ask about your setup" />
      <kbd>Enter</kbd>
    </form>
    <button class="nav" onclick={() => hub.openLibrary(null)} data-tip="The front page">Index</button>
    <button class="nav" onclick={() => hub.openLibrary("log.md")} data-tip="What changed, newest first">Log</button>
    <span class="find">Find <kbd>Ctrl O</kbd></span>
  {/snippet}

  {#if root}
    <div class="fill">
      <DocsView
        path={root}
        {version}
        doc={hub.libraryDoc}
        ongo={(t) => hub.openLibrary(t)}
        nav={false}
        empty="The library is empty. Ask it something above, and the answer is written down here."
      >
        {#snippet actions(page)}
          {@const understood = page.props.find((p) => p.key === "understood")?.value}
          {#if understood}
            <span class="got" data-tip="You passed a learning check on this">✓ understood {understood}</span>
          {/if}
          {#if !["index.md", "log.md"].includes(page.rel)}
            <button class="learn" onclick={() => hub.learn(page.title)} data-tip="Learn this">
              {understood ? "Learn again" : "Learn this"}
            </button>
          {/if}
        {/snippet}
      </DocsView>
    </div>
  {/if}
</Page>

<style>
  .ask { flex: 1; max-width: 720px; margin-left: 8px; display: flex; align-items: center; gap: 8px; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-md); padding: 0 10px; color: var(--muted); }
  .ask:focus-within { border-color: var(--line-strong); }
  .ask input { flex: 1; background: none; border: 0; outline: none; color: var(--text); font: inherit; font-size: 13px; padding: 8px 0; }
  .ask input::placeholder { color: var(--muted); }
  kbd { font: 11px var(--mono); border: 1px solid var(--line-strong); border-radius: var(--r-xs); padding: 0 4px; color: var(--muted); }
  .nav { color: var(--text-3); font-size: 13px; padding: 4px 8px; border-radius: var(--r-sm); }
  .nav:hover { background: var(--surface); color: var(--text); }
  .find { color: var(--muted); font-size: 12px; margin-left: auto; display: flex; gap: 6px; align-items: center; }
  .fill { flex: 1; min-height: 0; margin-top: 14px; }
  .got { color: var(--green); font-size: 12px; }
  .learn { color: var(--accent); border: 1px solid var(--accent); border-radius: var(--r-sm); padding: 1px 8px; font-size: 12px; }
  .learn:hover { background: var(--accent-bg); }
</style>
