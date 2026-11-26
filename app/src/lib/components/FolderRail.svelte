<!-- The left rail of the folder picker: the lists, the machines and the GitHub account.
     Used by FolderPicker.svelte. -->
<script lang="ts">
  import { hub } from "../state.svelte";
  import { gh } from "../github.svelte";
  import HostChip from "./HostChip.svelte";
  import Icon from "./Icon.svelte";
  import ProviderLogo from "./ProviderLogo.svelte";
  import Quiet from "./Quiet.svelte";

  type View = "recent" | "repos" | "folders" | "github" | { machine: string };
  let { counts, filter, view, viewMachine, rail, machineCount, openConnect }: {
    counts: { recent: number; repos: number; folders: number };
    filter: string;
    view: View;
    viewMachine: string | null;
    rail: (next: View) => void;
    machineCount: (host: string) => number;
    openConnect: () => void;
  } = $props();
</script>

<nav class="rail">
  {#each [["recent", "Recent", "clock", counts.recent], ["repos", "Repositories", "branch", counts.repos], ["folders", "Folders", "folder", counts.folders]] as const as [id, label, icon, n] (id)}
    <button class:on={!filter && view === id} onclick={(e) => { e.preventDefault(); rail(id); }}>
      <Icon name={icon} size={13} /><span class="grow">{label}</span><span class="n">{n}</span>
    </button>
  {/each}
  {#if hub.hosts.length > 1}
    <div class="rail-label">Machines</div>
    {#each hub.hosts as h (h.name)}
      <button
        class:on={!filter && viewMachine === h.name}
        disabled={h.state !== "online"}
        data-tip={h.state === "online" ? undefined : `${h.name} is ${h.state}${h.error ? `: ${h.error}` : ""}`}
        onclick={(e) => { e.preventDefault(); rail({ machine: h.name }); }}
      >
        <HostChip name={h.name} /><span class="grow"></span>
        {#if h.state === "online"}<span class="n">{machineCount(h.name)}</span>{/if}
        <span class="dot" class:up={h.state === "online"}></span>
      </button>
    {/each}
  {/if}
  <div class="rail-label">GitHub</div>
  {#if gh.state === "ready"}
    <button class:on={!filter && view === "github"} onclick={(e) => { e.preventDefault(); rail("github"); }}>
      <ProviderLogo id="github" size={13} /><span class="grow">All repos</span><span class="n">{gh.repos.filter((r) => !r.archived).length}</span>
    </button>
    {#if gh.login}<div class="rail-sub">{gh.login}</div>{/if}
  {:else if gh.state === "old"}
    <div class="rail-sub" data-tip="This Hub is older than the update that adds GitHub to the lists">Arrives with the next version of Hub</div>
  {:else if gh.state === "missing"}
    <div class="rail-sub" data-tip={gh.message ?? undefined}>No GitHub CLI (gh) found</div>
  {:else if gh.state === "none"}
    <button onclick={(e) => { e.preventDefault(); openConnect(); }}>
      <ProviderLogo id="github" size={13} /><span class="grow muted">Not connected</span>
    </button>
  {:else}
    <Quiet kind="loading" />
  {/if}
</nav>

<style>
  .rail { display: flex; flex-direction: column; gap: 1px; padding: 4px 2px 4px 4px; border-right: 1px solid var(--line); overflow-y: auto; max-height: 340px; min-height: 0; }
  .rail button { display: flex; align-items: center; gap: 8px; padding: 4px 8px; border-radius: var(--r-md); font-size: 13px; color: var(--text-3); text-align: left; }
  .rail button:hover:not(:disabled) { background: var(--surface-hover); color: var(--text); }
  .rail button.on { background: var(--surface-hover); color: var(--text); }
  .rail button:disabled { opacity: 0.55; }
  .rail button :global(svg) { color: var(--muted); flex: none; }
  .rail button.on :global(svg) { color: var(--accent); }
  .rail :global(.chip) { color: inherit; }
  .rail-label { color: var(--muted); font-size: 12px; padding: 10px 8px 3px; }
  .n { color: var(--muted); font-size: 12px; }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--line-strong); flex: none; }
  .dot.up { background: var(--green); }
  .rail-sub { color: var(--muted); font-size: 12px; padding: 0 8px 2px 29px; }
  .grow { flex: 1; }
  .muted { color: var(--muted); font-size: 12px; }
</style>
