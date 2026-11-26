<!-- The rows of the folder picker: projects, GitHub repositories, folders to step into and the ways to
     make or clone one. Used by FolderPicker.svelte. -->
<script lang="ts">
  import { ago } from "../format";
  import { hub } from "../state.svelte";
  import type { GithubRepo } from "../bindings/GithubRepo";
  import { away, parts, type Entry, type Row } from "../folderPicker";
  import HostChip from "./HostChip.svelte";
  import Icon from "./Icon.svelte";
  import IconSwap from "./IconSwap.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";
  import ProviderLogo from "./ProviderLogo.svelte";

  let {
    rows, at = $bindable(), host, filter, showMachine, here, machine, working,
    pick, into, bring, openGithub, run, pretty, bestClone, nameOf,
  }: {
    rows: Row[];
    /** The row the keyboard is on. */
    at: number;
    host: string | null;
    filter: string;
    showMachine: boolean;
    here: string;
    machine: string;
    working: string | null;
    pick: (path: string, on?: string | null) => void;
    into: (path: string) => void;
    bring: (e: Entry) => void;
    openGithub: (r: GithubRepo, local: Entry[]) => void;
    run: (row: Row) => void;
    pretty: (path: string) => string;
    bestClone: (es: Entry[]) => Entry;
    nameOf: (h: string | null) => string;
  } = $props();
  const hk = (h: string | null | undefined) => h ?? "";
</script>

{#snippet branchOf(e: Entry)}
  {#if e.branch}
    <span class="br" class:off={away(e)} data-tip={away(e) ? `On ${e.branch}, not ${e.main ?? "main"}` : `On ${e.branch}`}>
      <Icon name="branch" size={12} /><span class="bn">{e.branch}</span>
      {#if e.behind}<span class="behind" data-tip="{e.behind} commit{e.behind === 1 ? '' : 's'} on origin that this folder does not have (as of the last fetch)">↓{e.behind}</span>{/if}
    </span>
  {/if}
{/snippet}

{#each rows as r, i (r.kind === "project" ? `p${hk(r.e.host)}|${r.e.path}` : r.kind === "dir" ? `d${r.path}` : r.kind === "github" ? `g${r.r.full}` : r.kind)}
  {@const prev = rows[i - 1]}
  {#if r.kind === "project" && r.section && (prev?.kind !== "project" || prev.section !== r.section)}
    <div class="label">{r.section}</div>
  {/if}
  {#if r.kind === "github" && filter && !r.local.length && !(prev?.kind === "github" && !prev.local.length)}
    <div class="label">On GitHub, not cloned yet</div>
  {/if}
  {#if r.kind === "project"}
    {@const e = r.e}
    {@const there = !!host !== !!e.host || (host !== e.host)}
    <button
      class="row"
      class:on={at === i}
      data-tip="{e.path} on {e.machine}{e.key ? `\n${e.key}` : ''}{e.touched ? `\nlast change ${ago(e.touched)} ago` : ''}"
      onpointermove={() => (at = i)}
      onclick={(ev) => { ev.preventDefault(); pick(e.path, e.host); }}
    >
      <ProjectIcon path={e.path} host={e.host} name={e.name} size={16} />
      <span class="main">
        <span class="name">{#each parts(e.name, r.hits) as p}{#if p.hit}<b>{p.text}</b>{:else}{p.text}{/if}{/each}</span>
        <span class="path">{e.where}</span>
        {#if e.worktrees}<span class="wt">{e.worktrees} worktree{e.worktrees === 1 ? "" : "s"}</span>{/if}
      </span>
      {#if showMachine}
        <span class="mach" data-tip={r.also?.length ? `Also on ${r.also.map((a) => a.machine).join(", ")}` : undefined}>
          <HostChip name={e.machine} />{#each (r.also ?? []).slice(0, 1) as a (a.host)}<HostChip name={a.machine} />{/each}{#if (r.also?.length ?? 0) > 1}<span class="muted">+{r.also!.length - 1}</span>{/if}
        </span>
      {/if}
      <span class="brc">{@render branchOf(e)}</span>
      {#if e.repo && there}
        <span class="go" role="button" tabindex="-1" data-tip="Bring it to {nameOf(host)} first (history and .env files)" onclick={(ev) => { ev.preventDefault(); ev.stopPropagation(); bring(e); }} onkeydown={() => {}}>
          <Icon name={working === `${hk(e.host)}|${e.path}` ? "restart" : "download"} size={12} />
        </span>
      {/if}
    </button>
  {:else if r.kind === "github"}
    {@const g = r.r}
    <button
      class="row"
      class:on={at === i}
      data-tip={g.description ? `${g.full}\n${g.description}` : g.full}
      onpointermove={() => (at = i)}
      onclick={(ev) => { ev.preventDefault(); openGithub(g, r.local); }}
    >
      <span class="ghmark"><ProviderLogo id="github" size={15} /></span>
      <span class="main">
        <span class="name">{#each parts(g.name, r.hits) as p}{#if p.hit}<b>{p.text}</b>{:else}{p.text}{/if}{/each}</span>
        <span class="path">{g.full}</span>
        {#if g.private}<span class="priv" data-tip="Private"><Icon name="lock" size={11} /></span>{/if}
      </span>
      {#if r.local.length}
        <span class="mach"><HostChip name={bestClone(r.local).machine} />{#if r.local.length > 1}<span class="muted">+{r.local.length - 1}</span>{/if}</span>
        <span class="brc">{@render branchOf(bestClone(r.local))}</span>
      {:else}
        <span class="mach muted">only on GitHub</span>
        <span class="brc"><span class="clone" class:busy={working === g.full}>
          <IconSwap name={working === g.full ? "restart" : "download"} size={12} />{working === g.full ? "Cloning…" : `Clone to ${here === "this machine" ? "this machine" : here}`}
        </span></span>
      {/if}
    </button>
  {:else if r.kind === "dir"}
    <button
      class="row"
      class:on={at === i}
      data-tip={r.path}
      onpointermove={() => (at = i)}
      onclick={(ev) => { ev.preventDefault(); into(r.path); }}
      ondblclick={(ev) => { ev.preventDefault(); pick(r.path); }}
    >
      <Icon name={r.repo ? "branch" : "folder"} size={14} />
      <span class="main"><span class="name">{#each parts(r.name, r.hits) as p}{#if p.hit}<b>{p.text}</b>{:else}{p.text}{/if}{/each}</span></span>
      <span class="go" role="button" tabindex="-1" data-tip="Use {r.name}" onclick={(ev) => { ev.preventDefault(); ev.stopPropagation(); pick(r.path); }} onkeydown={() => {}}>
        <Icon name="check" size={12} />
      </span>
    </button>
  {:else if r.kind === "make"}
    <button class="row act" class:on={at === i} onpointermove={() => (at = i)} onclick={(ev) => { ev.preventDefault(); run(r); }}>
      <Icon name="plus" size={14} /><span class="main"><span class="name">New project <b>{r.name}</b> in {pretty(r.parent)}{hub.hosts.length > 1 ? ` on ${machine}` : ""}</span></span>
      <span class="muted">folder + git init</span>
    </button>
  {:else if r.kind === "clone"}
    <button class="row act" class:on={at === i} onpointermove={() => (at = i)} onclick={(ev) => { ev.preventDefault(); run(r); }}>
      <Icon name="download" size={14} /><span class="main"><span class="name">Clone into <b>{pretty(r.target)}</b></span></span>
      <span class="muted">git clone</span>
    </button>
  {/if}
{/each}

<style>
  .label { color: var(--muted); font-size: 12px; padding: 4px 8px 3px; }
  .row { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 8px; border-radius: var(--r-md); font-size: 13px; color: var(--text-2); text-align: left; }
  .row > :global(svg) { color: var(--muted); flex: none; }
  .row.on { background: var(--surface-hover); color: var(--text); }
  .main { display: flex; align-items: baseline; gap: 8px; min-width: 0; flex: 1; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: none; max-width: 55%; }
  .name b { font-weight: 600; color: var(--text); }
  .path { color: var(--muted); font-family: var(--mono); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .wt { color: var(--muted); font-size: 12px; flex: none; }
  .wt::before { content: "· "; }
  .mach { flex: none; min-width: 74px; max-width: 190px; font-size: 12px; overflow: hidden; }
  .brc { flex: none; width: 158px; display: flex; justify-content: flex-end; }
  .br { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-family: var(--mono); font-size: 12px; min-width: 0; }
  .br :global(svg) { flex: none; }
  .br .bn { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .br.off { color: var(--amber); }
  .behind { color: var(--accent); flex: none; }
  .go { display: none; color: var(--muted); padding: 2px 4px; border-radius: var(--r-sm); flex: none; }
  .row.on .go { display: inline-grid; }
  .go:hover { color: var(--text); background: var(--surface); }
  .act { border-top: 1px solid var(--line); border-radius: 0 0 var(--r-md) var(--r-md); margin-top: 3px; padding-top: 6px; }
  .row:disabled { opacity: 0.6; }
  .ghmark { display: grid; place-items: center; width: 16px; flex: none; opacity: 0.7; }
  .priv { color: var(--muted); flex: none; display: inline-grid; }
  .mach { display: inline-flex; align-items: center; gap: 6px; }
  .clone { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--accent); border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent); background: var(--accent-bg); padding: 2px 8px; border-radius: var(--r-sm); white-space: nowrap; }
  .clone.busy { opacity: 0.7; }
</style>
