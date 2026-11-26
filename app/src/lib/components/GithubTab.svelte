<!-- The project's GitHub tab, after T3 Code's Pull requests page: pull requests, issues and branches listed on the left, the one picked on the right. Pull requests are grouped by how close they are to merging, and each says which thread made it.
     Used by ProjectView.svelte. -->
<script lang="ts">
  import { autofocus } from "../autofocus";
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  // The project's GitHub tab, after T3 Code's Pull requests page: pull requests, issues and
  // branches listed on the left, the one picked on the right. Pull requests are grouped by how
  // close they are to merging, and each says which thread made it.
  import { api } from "../api";
  import type { BranchList } from "../bindings/BranchList";
  import type { IssueList } from "../bindings/IssueList";
  import type { PullList } from "../bindings/PullList";
  import { ago } from "../format";
  import { checksLabel, groupPulls, reviewLabel } from "../github";
  import { menu } from "../menu.svelte";
  import { linkMenu } from "../menus.svelte";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import IssueView from "./IssueView.svelte";
  import PullView from "./PullView.svelte";

  let {
    path,
    host = null,
    onthread,
  }: { path: string; host?: string | null; onthread?: (text: string) => void } = $props();

  // What is listed and what is open, kept on hub so the Overview card can open a pull request here.
  const mode = $derived(hub.github.mode);
  let listed = $state<"open" | "merged" | "closed">("open");
  let query = $state("");

  let pulls = $state<PullList | null>(null);
  let issues = $state<IssueList | null>(null);
  let branches = $state<BranchList | null>(null);
  let branchError = $state<string | null>(null);

  const loadPulls = () => api.pulls(path, listed, host).then((l) => (pulls = l)).catch((e) => (pulls = { message: String(e), pulls: [] }));
  const loadIssues = () => api.issues(path, listed === "open" ? "open" : "closed", host).then((l) => (issues = l)).catch((e) => (issues = { message: String(e), issues: [] }));
  const loadBranches = () =>
    api.branchList(path, host).then((b) => ((branches = b), (branchError = null))).catch((e) => (branchError = e instanceof Error ? e.message : String(e)));
  function reload() {
    if (mode === "pulls") return loadPulls();
    if (mode === "issues") return loadIssues();
    return loadBranches();
  }
  $effect(() => {
    void [mode, listed, path];
    reload();
    const timer = poll(reload, mode === "branches" ? 120_000 : 60_000);
    return () => timer();
  });
  // A closed or merged one picked from the Overview card shows in its own list.
  $effect(() => {
    if (listed === "merged" && mode === "issues") listed = "closed";
  });

  const q = $derived(query.trim().toLowerCase());
  const match = (n: number, ...texts: string[]) => !q || `#${n}`.includes(q) || texts.some((t) => t.toLowerCase().includes(q));
  const shownPulls = $derived(pulls?.pulls.filter((p) => match(p.number, p.title, p.author, p.head, ...p.labels)) ?? []);
  const groups = $derived(groupPulls(shownPulls));
  const shownIssues = $derived(issues?.issues.filter((i) => match(i.number, i.title, i.author, ...i.labels)) ?? []);
  const threadFor = (url: string) => {
    const id = Object.entries(hub.prs).find(([, pr]) => pr.url === url)?.[0];
    return id ? hub.threads.find((t) => t.id === id) : undefined;
  };

  // New issue, written in place.
  let writing = $state(false);
  let newTitle = $state("");
  let newBody = $state("");
  async function createIssue(e: SubmitEvent) {
    e.preventDefault();
    try {
      const n = await api.issueAction({ path, number: 0, action: "create", title: newTitle, body: newBody }, host);
      writing = false;
      newTitle = newBody = "";
      listed = "open";
      await loadIssues();
      hub.github.issue = n;
    } catch (err) {
      hub.fail(err);
    }
  }

  // Branches: tidy the merged ones away in one go.
  const merged = $derived(branches?.branches.filter((b) => b.merged && !b.current && b.name !== branches!.default) ?? []);
  const shownBranches = $derived(branches?.branches.filter((b) => !q || b.name.toLowerCase().includes(q) || b.subject.toLowerCase().includes(q)) ?? []);
  let deleting = $state(false);
  async function deleteBranches(names: string[], local: boolean, remote: boolean) {
    deleting = true;
    try {
      const failed = await api.deleteBranches(path, names, local, remote, host);
      if (failed.length) hub.fail(new Error(failed.join("; ")));
      await loadBranches();
    } catch (e) {
      hub.fail(e);
    } finally {
      deleting = false;
    }
  }
  const pullOfBranch = (name: string) => pulls?.pulls.find((p) => p.head === name);
  $effect(() => {
    // The branch list says which pull request each branch has.
    if (mode === "branches" && !pulls) loadPulls();
  });
</script>

<div class="gh">
  <div class="list">
    <div class="seg modes">
      <button class:on={mode === "pulls"} onclick={() => (hub.github.mode = "pulls")}><Icon name="pr" size={13} />Pull requests</button>
      <button class:on={mode === "issues"} onclick={() => (hub.github.mode = "issues")}>Issues</button>
      <button class:on={mode === "branches"} onclick={() => (hub.github.mode = "branches")}><Icon name="branch" size={13} />Branches</button>
    </div>
    <div class="filters">
      <input class="search" bind:value={query} placeholder="Search…" spellcheck="false" />
      {#if mode !== "branches"}
        <div class="seg">
          <button class:on={listed === "open"} onclick={() => (listed = "open")}>Open</button>
          {#if mode === "pulls"}<button class:on={listed === "merged"} onclick={() => (listed = "merged")}>Merged</button>{/if}
          <button class:on={listed === "closed"} onclick={() => (listed = "closed")}>Closed</button>
        </div>
      {/if}
    </div>

    {#if mode === "pulls"}
      {#if !pulls}
        <Quiet kind="loading" />
      {:else if pulls.message}
        <p class="none">{pulls.message}</p>
      {:else}
        {#each groups as g (g.key)}
          <div class="label group">{g.label}<span class="n">{g.pulls.length}</span></div>
          {#each g.pulls as p (p.number)}
            {@const t = threadFor(p.url)}
            <button class="row" class:on={hub.github.pull === p.number} onclick={() => (hub.github.pull = p.number)} use:menu={() => linkMenu(p.url)}>
              <span class="ic {p.state}" class:draft={p.draft}></span>
              <span class="grow">
                <span class="t">{p.title}</span>
                <span class="m">
                  <span class="mono">#{p.number}</span>
                  {#if p.state === "open"}
                    {#if p.checks !== "none"}<span class="c {p.checks}">● {checksLabel[p.checks]}</span>{/if}
                    {#if p.mergeable === "conflicts"}<span class="c fail">● conflicts</span>{/if}
                    {#if reviewLabel[p.review]}<span class="c {p.review}">{reviewLabel[p.review]}</span>{/if}
                  {/if}
                  <span><span class="add">+{p.additions}</span> <span class="del">−{p.deletions}</span></span>
                  <span>{ago(p.updated_at)}</span>
                  {#if t}<span class="thread" data-tip="Made in the thread {t.title}"><span class="dot {t.status}"></span>{t.title}</span>{:else if p.author}<span>{p.author}</span>{/if}
                </span>
              </span>
            </button>
          {/each}
        {:else}
          <p class="none">{q ? "Nothing matches." : listed === "open" ? "No open pull requests." : `No ${listed} pull requests.`}</p>
        {/each}
      {/if}
    {:else if mode === "issues"}
      {#if listed === "open"}
        {#if writing}
          <form class="new" onsubmit={createIssue}>
            <input bind:value={newTitle} placeholder="Title" use:autofocus />
            <textarea bind:value={newBody} placeholder="What is it about? (optional)" rows="3"></textarea>
            <div class="new-row"><button type="button" class="btn ghost" onclick={() => (writing = false)}>Cancel</button><button class="btn primary" disabled={!newTitle.trim()}>Create issue</button></div>
          </form>
        {:else}
          <button class="add-row" onclick={() => (writing = true)}><Icon name="plus" size={13} />New issue</button>
        {/if}
      {/if}
      {#if !issues}
        <Quiet kind="loading" />
      {:else if issues.message}
        <p class="none">{issues.message}</p>
      {:else}
        {#each shownIssues as i (i.number)}
          <button class="row" class:on={hub.github.issue === i.number} onclick={() => (hub.github.issue = i.number)} use:menu={() => linkMenu(i.url)}>
            <span class="ic issue {i.state}"></span>
            <span class="grow">
              <span class="t">{i.title}</span>
              <span class="m"><span class="mono">#{i.number}</span>{#each i.labels as l (l)}<span class="chip">{l}</span>{/each}{#if i.comments}<span>{i.comments} {i.comments === 1 ? "comment" : "comments"}</span>{/if}<span>{ago(i.updated_at)}</span></span>
            </span>
          </button>
        {:else}
          <p class="none">{q ? "Nothing matches." : `No ${listed} issues.`}</p>
        {/each}
      {/if}
    {:else}
      {#if branchError}
        <p class="none">{branchError}</p>
      {:else if !branches}
        <Quiet kind="loading" />
      {:else}
        {#if merged.length}
          <div class="tidy">
            <span>{merged.length} merged {merged.length === 1 ? "branch" : "branches"}</span>
            <button class="link" disabled={deleting} onclick={() => deleteBranches(merged.map((b) => b.name), true, true)}>Delete them here and on GitHub</button>
          </div>
        {/if}
        {#each shownBranches as b (b.name)}
          {@const pr = pullOfBranch(b.name)}
          <div class="row branch" use:menu={() => [
            pr && { label: `Open pull request #${pr.number}`, run: () => ((hub.github.mode = "pulls"), (hub.github.pull = pr.number)) },
            b.name !== branches!.default && !b.current && b.local && { label: "Delete here", danger: true, run: () => deleteBranches([b.name], true, false) },
            b.name !== branches!.default && b.remote && { label: "Delete on GitHub", danger: true, confirm: "Delete it on GitHub?", run: () => deleteBranches([b.name], false, true) },
          ]}>
            <span class="grow">
              <span class="t mono">{b.name}{#if b.current}<span class="here">checked out</span>{/if}</span>
              <span class="m">
                {#if b.name === branches.default}<span>default</span>{:else if b.merged}<span class="c merged">merged</span>{:else if b.ahead}<span>{b.ahead} ahead</span>{/if}
                {#if pr}<button class="c link-pr" onclick={() => ((hub.github.mode = "pulls"), (hub.github.pull = pr.number))}>#{pr.number} {pr.state}</button>{/if}
                <span>{b.local && b.remote ? "here and on GitHub" : b.local ? "only here" : "only on GitHub"}</span>
                <span>{ago(b.at)}</span>
              </span>
              <span class="m subject" data-tip={b.subject}>{b.subject}</span>
            </span>
          </div>
        {/each}
      {/if}
    {/if}
  </div>

  <div class="detail">
    {#if mode === "pulls" && hub.github.pull}
      {#key hub.github.pull}<PullView {path} {host} number={hub.github.pull} {onthread} onchange={loadPulls} />{/key}
    {:else if mode === "issues" && hub.github.issue}
      {#key hub.github.issue}<IssueView {path} {host} number={hub.github.issue} {onthread} onchange={loadIssues} />{/key}
    {:else if mode === "branches"}
      <p class="none">Right-click a branch to delete it here or on GitHub. Merged means everything on it is in {branches?.default ?? "the default branch"} already.</p>
    {:else}
      <p class="none">Pick one on the left.</p>
    {/if}
  </div>
</div>

<style>
  .gh { display: grid; grid-template-columns: minmax(300px, 380px) minmax(0, 1fr); gap: 16px; align-items: start; }
  @media (max-width: 900px) { .gh { grid-template-columns: minmax(0, 1fr); } }
  .list { background: var(--surface); border-radius: var(--r-lg); padding: 10px; position: sticky; top: 0; max-height: calc(100vh - 260px); overflow-y: auto; }
  .detail { background: var(--surface); border-radius: var(--r-lg); padding: 18px 22px; min-height: 300px; min-width: 0; }
  .seg { display: flex; border: 1px solid var(--line-strong); border-radius: var(--r-md); overflow: hidden; flex: none; }
  .seg button { padding: 3px 10px; font-size: 13px; color: var(--muted); display: inline-flex; align-items: center; gap: 4px; }
  .seg button:hover { color: var(--text-2); }
  .seg button.on { background: var(--surface-hover); color: var(--text); }
  .modes { margin-bottom: 8px; }
  .modes button { flex: 1; justify-content: center; }
  .filters { display: flex; gap: 6px; margin-bottom: 6px; }
  .search { flex: 1; min-width: 0; background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 3px 8px; font: inherit; font-size: 13px; color: var(--text-2); outline: none; }
  .search:focus { border-color: var(--accent); }
  .group { display: flex; gap: 6px; padding: 10px 8px 3px; }
  .n { color: var(--muted); }
  .row { display: flex; gap: 10px; width: 100%; padding: 8px 8px; border-radius: var(--r-md); text-align: left; align-items: flex-start; }
  .row:hover, .row.on { background: var(--surface-hover); }
  .row.branch { cursor: default; }
  .grow { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .t { color: var(--text-2); font-size: 13px; line-height: 1.35; }
  .row.on .t { color: var(--text); }
  .m { display: flex; flex-wrap: wrap; gap: 2px 8px; align-items: center; color: var(--muted); font-size: 12px; }
  .m .mono { font-size: 11px; }
  .subject { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ic { width: 13px; height: 13px; border-radius: 50%; margin-top: 3px; flex: none; border: 2px solid var(--green); }
  .ic.draft { border-color: var(--muted); border-style: dashed; }
  .ic.merged { border-color: var(--accent); background: var(--accent); }
  .ic.closed { border-color: var(--red); }
  .ic.issue.closed { border-color: var(--accent); background: var(--accent); }
  .c.pass, .c.approved { color: var(--green); }
  .c.fail { color: var(--red); }
  .c.pending, .c.changes_requested { color: var(--amber); }
  .c.merged { color: var(--accent); }
  .add { color: var(--green); }
  .del { color: var(--red); }
  .thread { display: inline-flex; align-items: center; gap: 4px; max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-3); }
  .chip { background: var(--surface-hover); color: var(--text-3); border-radius: var(--r-sm); padding: 0 6px; }
  .here { margin-left: 8px; font-family: var(--font); font-size: 11px; color: var(--green); }
  .link-pr { color: var(--accent); font-size: inherit; padding: 0; }
  .none { color: var(--muted); font-size: 13px; margin: 8px; overflow-wrap: anywhere; }
  .detail .none { margin: 0; }
  .add-row { display: flex; align-items: center; gap: 8px; width: 100%; padding: 8px 8px; color: var(--muted); font-size: 13px; border-radius: var(--r-md); }
  .add-row:hover { color: var(--text); background: var(--surface-hover); }
  .new { display: flex; flex-direction: column; gap: 6px; padding: 6px 4px 10px; border-bottom: 1px solid var(--line); margin-bottom: 4px; }
  .new input, .new textarea { background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 4px 8px; font: inherit; font-size: 13px; color: var(--text-2); outline: none; resize: vertical; }
  .new-row { display: flex; justify-content: flex-end; gap: 6px; }
  .ghost { background: none; }
  .tidy { display: flex; align-items: center; gap: 10px; padding: 4px 8px 8px; font-size: 12px; color: var(--text-3); border-bottom: 1px solid var(--line); margin-bottom: 4px; }
  .tidy span { flex: 1; }
  .link { color: var(--accent); font-size: 12px; padding: 0; }
  .link:disabled { opacity: 0.5; }
</style>
