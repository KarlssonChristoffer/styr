<!-- One pull request: what it is, its checks, merging, the conversation with a box to answer or review in, its files as a diff and its commits. On the project's GitHub tab, and in the right panel when opened from a thread (`narrow`). Everything goes through the core's gh.
     Used by App.svelte, GithubTab.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  import { fingerprint, isTest, setViewed, viewedIn } from "../viewed";
  // One pull request: what it is, its checks, merging, the conversation with a box to answer or
  // review in, its files as a diff and its commits. On the project's GitHub tab, and in the right
  // panel when opened from a thread (`narrow`). Everything goes through the core's gh.
  import { api } from "../api";
  import type { PullDetail } from "../bindings/PullDetail";
  import type { PullComment } from "../bindings/PullComment";
  import { ago, shortVersion } from "../format";
  import { checksLabel, parsePatch, pullTone, reviewLabel, type PatchFile } from "../github";
  import { contextMenu, copy, menu } from "../menu.svelte";
  import { linkMenu } from "../menus.svelte";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import Switch from "./Switch.svelte";
  import Markdown from "./Markdown.svelte";

  let {
    path,
    host = null,
    number,
    narrow = false,
    onthread,
    onchange,
    onclose,
  }: {
    path: string;
    host?: string | null;
    number: number;
    narrow?: boolean;
    /** Starts a thread in the project with this first message. */
    onthread?: (text: string) => void;
    /** Something was done to it, so lists can refresh. */
    onchange?: () => void;
    onclose?: () => void;
  } = $props();

  type Tab = "summary" | "conversation" | "files" | "commits";
  let tab = $state<Tab>("summary");
  let d = $state<PullDetail | null>(null);
  let error = $state<string | null>(null);
  let busy = $state<string | null>(null);

  async function load() {
    try {
      const got = await api.pull(path, number, host);
      if (got.pull.number === number) d = got;
      error = null;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  $effect(() => {
    void number;
    d = null;
    patch = null;
    tab = "summary";
    load();
    const timer = poll(load, 30_000);
    return () => timer();
  });

  // The diff is only fetched when the Files tab opens.
  let patch = $state<PatchFile[] | null>(null);
  let patchError = $state<string | null>(null);
  let openFiles = $state<Record<string, boolean>>({});
  // Ticked off as read: folded, counted, and back as changed when the file moves on.
  const prKey = $derived(`${host ?? ""}|${path}#${number}`);
  let viewed = $state<Record<string, string>>({});
  let hideTests = $state(false);
  const prints = $derived(Object.fromEntries((patch ?? []).map((f) => [f.path, fingerprint(f.lines.map((l) => l.op + l.text).join("\n"))])));
  const readMark = (path: string) => (!viewed[path] ? "" : viewed[path] === prints[path] ? "viewed" : "changed");
  const shownFiles = $derived((patch ?? []).filter((f) => !hideTests || !isTest(f.path)));
  const testCount = $derived((patch ?? []).filter((f) => isTest(f.path)).length);
  const viewedCount = $derived((patch ?? []).filter((f) => readMark(f.path) === "viewed").length);
  function tick(path: string) {
    const on = readMark(path) !== "viewed";
    setViewed(prKey, path, on ? prints[path] : null);
    viewed = viewedIn(prKey);
    openFiles[path] = !on;
  }
  $effect(() => {
    if (tab !== "files" || patch) return;
    const n = number;
    api
      .pullDiff(path, n, host)
      .then((r) => {
        if (n !== number) return;
        patch = parsePatch(r.text);
        viewed = viewedIn(prKey);
        // Small pull requests open every file; big ones start folded. Read ones stay folded.
        openFiles = Object.fromEntries(patch.map((f) => [f.path, patch!.length <= 8 && readMark(f.path) !== "viewed"]));
      })
      .catch((e) => (patchError = e instanceof Error ? e.message : String(e)));
  });

  async function act(action: string, extra: { method?: string; body?: string; delete_branch?: boolean } = {}) {
    busy = action;
    try {
      await api.pullAction({ path, number, action, method: extra.method ?? null, body: extra.body ?? null, delete_branch: extra.delete_branch ?? false }, host);
      await load();
      onchange?.();
      return true;
    } catch (e) {
      hub.fail(e);
      return false;
    } finally {
      busy = null;
    }
  }

  // Merging: the repository's default method first; one more click to be sure.
  let method = $state<string | null>(null);
  const methodNow = $derived(method ?? d?.methods[0] ?? "squash");
  const methodName: Record<string, string> = { squash: "Squash and merge", merge: "Merge", rebase: "Rebase and merge" };
  let deleteBranch = $state(true);
  let armed = $state(false);
  $effect(() => {
    if (!armed) return;
    const t = setTimeout(() => (armed = false), 4000);
    return () => clearTimeout(t);
  });
  async function merge() {
    if (!armed) return (armed = true);
    armed = false;
    await act("merge", { method: methodNow, delete_branch: deleteBranch && !d?.deletes_branch });
  }
  function pickMethod(e: MouseEvent) {
    at(e, (d?.methods ?? []).map((m) => ({ label: methodName[m] ?? m, hint: m === methodNow ? "✓" : undefined, run: () => (method = m) })));
  }
  function at(e: MouseEvent, items: Parameters<typeof contextMenu.show>[2]) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const zoom = Number(document.documentElement.style.zoom) || 1;
    contextMenu.show(r.left / zoom, r.bottom / zoom + 4, items);
  }
  function more(e: MouseEvent) {
    if (!d) return;
    const p = d.pull;
    at(e, [
      { label: "Open on GitHub", run: () => openUrl(p.url) },
      { label: "Copy link", run: () => copy(p.url) },
      { label: "Copy branch name", run: () => copy(p.head) },
      null,
      onthread && { label: "Work on it in a thread", run: () => workOn() },
      p.state === "open" && { label: "Check out in the project folder", hint: p.head, run: () => act("checkout"), confirm: "Switch the folder's branch?" },
      p.state === "open" && d.merge_state === "behind" && { label: `Update branch from ${p.base}`, run: () => act("update_branch") },
      p.state === "open" && (p.draft ? { label: "Ready for review", run: () => act("ready") } : { label: "Convert to draft", run: () => act("draft") }),
      null,
      p.state === "open" && { label: "Close", danger: true, run: () => act("close") },
      p.state === "closed" && { label: "Reopen", run: () => act("reopen") },
      p.state !== "open" && { label: "Delete branch on GitHub", hint: p.head, danger: true, confirm: "Delete it?", run: () => act("delete_branch") },
    ]);
  }

  function workOn() {
    if (!d) return;
    const p = d.pull;
    onthread?.(`Work on pull request #${p.number} "${p.title}" (${p.url}). Check out its branch \`${p.head}\` first (\`gh pr checkout ${p.number}\`), and push your commits to that branch so they land in the pull request.\n\n`);
  }
  function fixIt(c: PullComment) {
    if (!d) return;
    const p = d.pull;
    const where = c.path ? ` on \`${c.path}${c.line ? `:${c.line}` : ""}\`` : "";
    onthread?.(
      `Fix this review comment on pull request #${p.number} "${p.title}" (${p.url}).\n\n${c.author} wrote${where}:\n\n> ${c.body.trim().replace(/\n/g, "\n> ")}\n\nCheck out the branch \`${p.head}\` first (\`gh pr checkout ${p.number}\`), fix it, run the checks, and push to that branch. Then answer the comment on the pull request saying what you did.`,
    );
  }

  // Writing back: comment, approve or ask for changes.
  let reply = $state("");
  async function send(action: "comment" | "approve" | "request_changes") {
    if (await act(action, { body: reply })) reply = "";
  }
  function replyKey(e: KeyboardEvent) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      if (reply.trim()) send("comment");
    }
  }

  const tone = $derived(d ? pullTone(d.pull) : null);
  const failing = $derived(d?.checks.filter((c) => c.state === "fail").length ?? 0);
  const mergeNote = $derived.by(() => {
    if (!d) return null;
    const p = d.pull;
    if (p.mergeable === "conflicts" || d.merge_state === "dirty") return { tone: "bad", text: `Conflicts with ${p.base}` };
    if (d.merge_state === "behind") return { tone: "warn", text: `Behind ${p.base}` };
    if (p.checks === "fail") return { tone: "bad", text: `${failing} ${failing === 1 ? "check fails" : "checks fail"}` };
    if (p.review === "changes_requested") return { tone: "warn", text: "Changes asked for" };
    if (d.merge_state === "blocked") return { tone: "warn", text: p.review === "review_required" ? "Needs an approving review" : "Blocked by branch rules" };
    if (p.checks === "pending") return { tone: "warn", text: "Checks still running" };
    if (p.draft) return { tone: "warn", text: "Draft" };
    return { tone: "ok", text: `No conflicts with ${p.base}` };
  });
  const threadOf = $derived(d ? Object.entries(hub.prs).find(([, pr]) => pr.url === d!.pull.url)?.[0] : undefined);
  const threadTitle = $derived(threadOf ? hub.threads.find((t) => t.id === threadOf)?.title : undefined);
  const verdict: Record<string, string> = { approved: "approved", changes_requested: "asked for changes", commented: "reviewed", dismissed: "review dismissed" };
  const fileCount = $derived(d?.files.length ?? 0);
</script>

{#snippet said(c: PullComment)}
  <div class="said" class:review={c.kind === "review"}>
    <div class="who">
      <span class="author">{c.author}</span>
      {#if c.kind === "review"}
        <span class="verdict {c.verdict}">{verdict[c.verdict ?? ""] ?? c.verdict}</span>
      {:else if c.kind === "line"}
        <span>on <span class="mono">{c.path}{c.line ? `:${c.line}` : ""}</span></span>
      {/if}
      <span class="when">{ago(c.at)}</span>
      {#if onthread && c.body.trim() && d?.pull.state === "open"}
        <button class="fix" onclick={() => fixIt(c)} data-tip={narrow ? "Ask this thread's agent to fix it" : "Start a thread that fixes this"}>{narrow ? "Ask the agent to fix" : "Fix in a thread"}</button>
      {/if}
    </div>
    {#if c.body.trim()}<div class="body"><Markdown text={c.body} /></div>{/if}
  </div>
{/snippet}

<div class="pull" class:narrow>
  {#if error && !d}
    <p class="none">{error}</p>
  {:else if !d}
    <p class="none">Loading #{number}…</p>
  {:else}
    {@const p = d.pull}
    <header>
      <div class="title-row">
        <h2 use:menu={() => linkMenu(p.url)}>{p.title} <span class="num">#{p.number}</span></h2>
        <button class="icon" data-tip="More" onclick={more}><Icon name="more" size={15} /></button>
        {#if onclose}<button class="icon" data-tip="Close" onclick={onclose}><Icon name="x" size={15} /></button>{/if}
      </div>
      <div class="meta">
        <span class="state {tone?.tone}">{tone?.label}</span>
        <span class="mono refs" data-tip="{p.head} into {p.base}">{p.head} → {p.base}</span>
        <span>by {p.author}{#if threadTitle}, from <button class="link" onclick={() => hub.open(threadOf!)}>{threadTitle}</button>{/if}</span>
        <span class="add">+{p.additions}</span><span class="del">−{p.deletions}</span>
      </div>
    </header>

    <nav class="tabs">
      <button class:on={tab === "summary"} onclick={() => (tab = "summary")}>Summary</button>
      <button class:on={tab === "conversation"} onclick={() => (tab = "conversation")}>Conversation{#if d.comments.length}<span class="n">{d.comments.length}</span>{/if}</button>
      <button class:on={tab === "files"} onclick={() => (tab = "files")}>Files<span class="n">{fileCount}</span></button>
      <button class:on={tab === "commits"} onclick={() => (tab = "commits")}>Commits<span class="n">{d.commits.length}</span></button>
    </nav>

    {#if tab === "summary"}
      <div class="cols">
        <div class="main">
          {#if d.body.trim()}<div class="desc"><Markdown text={d.body} /></div>{:else}<Quiet text="No description" />{/if}
          {#if d.comments.length}
            <button class="latest" onclick={() => (tab = "conversation")}>
              Latest: <b>{d.comments.at(-1)!.author}</b> {d.comments.at(-1)!.kind === "review" ? verdict[d.comments.at(-1)!.verdict ?? ""] ?? "reviewed" : "commented"} {ago(d.comments.at(-1)!.at)} ago · See all {d.comments.length}
            </button>
          {/if}
        </div>
        <div class="side">
          {#if p.state === "open"}
            <section>
              <div class="label">Merge</div>
              {#if mergeNote}<div class="line {mergeNote.tone}"><span class="dot"></span>{mergeNote.text}</div>{/if}
              <div class="merge-row">
                <button class="btn merge" class:armed disabled={!!busy || p.draft} onclick={merge}>{busy === "merge" ? "Merging…" : armed ? "Click again to merge" : methodName[methodNow] ?? "Merge"}</button>
                {#if d.methods.length > 1}<button class="btn caret" data-tip="How to merge" onclick={pickMethod}><Icon name="down" size={12} /></button>{/if}
              </div>
              <label class="opt"><Switch label="Delete the branch" on={deleteBranch} disabled={d.deletes_branch} onchange={(on) => (deleteBranch = on)} />{d.deletes_branch ? "GitHub deletes the branch" : "Delete the branch"}</label>
              {#if d.auto_merge}
                <div class="line ok"><span class="dot"></span>Merges by itself when ready <button class="link" disabled={!!busy} onclick={() => act("cancel_auto_merge")}>Cancel</button></div>
              {:else if mergeNote?.tone !== "ok"}
                <button class="link" disabled={!!busy} onclick={() => act("auto_merge", { method: methodNow })} data-tip="GitHub merges it once its required checks and reviews are in. Needs auto-merge turned on for the repository.">Merge by itself when ready</button>
              {/if}
            </section>
          {:else if p.state === "merged"}
            <section><div class="line merged"><span class="dot"></span>Merged {d.merged_at ? `${ago(d.merged_at)} ago` : ""}</div></section>
          {/if}
          <section>
            <div class="label">Checks</div>
            {#each d.checks as c (c.name)}
              <button class="check" disabled={!c.url} onclick={() => c.url && openUrl(c.url)} data-tip={c.url ?? ""}><span class="dot {c.state}"></span><span class="grow">{c.name}</span></button>
            {:else}
              <Quiet text="No checks on GitHub" />
            {/each}
          </section>
          <section>
            <div class="label">Review</div>
            <div class="line">{reviewLabel[p.review] ?? "Not needed"}</div>
            {#if d.reviewers.length}<div class="line muted">Waiting on {d.reviewers.join(", ")}</div>{/if}
            {#if p.labels.length}<div class="chips">{#each p.labels as l (l)}<span class="chip">{l}</span>{/each}</div>{/if}
          </section>
        </div>
      </div>
    {:else if tab === "conversation"}
      <div class="talk">
        {#each d.comments as c, i (i)}{@render said(c)}{:else}<Quiet text="Nothing said" />{/each}
      </div>
    {:else if tab === "files"}
      {#if patchError}
        <p class="none">{patchError}</p>
      {:else if !patch}
        <Quiet kind="loading" />
      {:else}
        <div class="files-bar">
          <span>{patch.length} {patch.length === 1 ? "file" : "files"} · {viewedCount} viewed</span>
          {#if testCount}<button class="link" aria-pressed={hideTests} onclick={() => (hideTests = !hideTests)}>{hideTests ? `Show ${testCount} test ${testCount === 1 ? "file" : "files"}` : "Hide tests"}</button>{/if}
          <button class="link" onclick={() => (openFiles = Object.fromEntries(patch!.map((f) => [f.path, true])))}>Open all</button>
          <button class="link" onclick={() => (openFiles = {})}>Fold all</button>
        </div>
        {#each shownFiles as f (f.path)}
          {@const mark = readMark(f.path)}
          <div class="file" class:read={mark === "viewed"}>
            <button class="file-head" onclick={() => (openFiles[f.path] = !openFiles[f.path])}>
              <span class="chev" class:open={openFiles[f.path]}><Icon name="chevron" size={12} /></span>
              <span class="mono grow" data-tip={f.old ? `${f.old} → ${f.path}` : f.path}>{f.old ? `${f.old} → ` : ""}{f.path}</span>
              {#if mark === "changed"}<span class="changed-mark" data-tip="Changed since you ticked it off">Changed</span>{/if}
              <span class="add">+{f.additions}</span><span class="del">−{f.deletions}</span>
              <span
                class="tick"
                class:on={mark === "viewed"}
                role="checkbox"
                tabindex="0"
                aria-checked={mark === "viewed"}
                data-tip={mark === "viewed" ? "Viewed: click to unmark" : "Mark as viewed"}
                onclick={(e) => { e.stopPropagation(); tick(f.path); }}
                onkeydown={(e) => { if (e.key === " " || e.key === "Enter") { e.preventDefault(); e.stopPropagation(); tick(f.path); } }}
              >{#if mark === "viewed"}<Icon name="check" size={11} />{/if}</span>
            </button>
            {#if openFiles[f.path]}
              {#if f.binary}<p class="none small">Binary file.</p>{:else}
                <pre>{#each f.lines as l, k (k)}<span class="l {l.op === '+' ? 'plus' : l.op === '-' ? 'minus' : l.op === '@' ? 'hunk' : ''}">{l.op === "@" ? "⋯ " : `${l.op} `}{l.text}
</span>{/each}</pre>
              {/if}
            {/if}
          </div>
        {/each}
      {/if}
    {:else}
      <div class="commits">
        {#each d.commits as c (c.oid)}
          <div class="commit"><span class="mono hash">{shortVersion(c.oid)}</span><span class="grow">{c.headline}</span><span class="when">{c.author} · {ago(c.at)}</span></div>
        {/each}
      </div>
    {/if}

    {#if tab === "summary" || tab === "conversation"}
      <div class="reply">
        <textarea bind:value={reply} onkeydown={replyKey} placeholder="Comment, or say what to change…" rows="2"></textarea>
        <div class="reply-row">
          <span class="hint">Ctrl Enter comments</span>
          <button class="btn" disabled={!reply.trim() || !!busy} onclick={() => send("comment")}>Comment</button>
          {#if p.state === "open"}
            <button class="btn" disabled={!!busy} onclick={() => send("approve")} data-tip="Approve. GitHub does not let you approve your own pull request.">Approve</button>
            <button class="btn" disabled={!reply.trim() || !!busy} onclick={() => send("request_changes")}>Request changes</button>
          {/if}
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .pull { display: flex; flex-direction: column; min-width: 0; }
  header { margin-bottom: 10px; }
  .title-row { display: flex; align-items: flex-start; gap: 6px; }
  h2 { flex: 1; min-width: 0; color: var(--text); font-size: 16px; font-weight: 500; margin: 0; line-height: 1.35; }
  .num { color: var(--muted); font-weight: 400; }
  .icon { display: grid; place-items: center; width: 26px; height: 26px; border-radius: var(--r-sm); color: var(--muted); flex: none; }
  .icon:hover { color: var(--text); background: var(--surface-hover); }
  .meta { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 10px; color: var(--muted); font-size: 12px; margin-top: 6px; }
  .refs { color: var(--text-3); font-size: 12px; max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .state { border-radius: var(--r-sm); padding: 0 8px; font-size: 12px; }
  .state.open { background: var(--green-bg); color: var(--green); }
  .state.draft { background: var(--surface-hover); color: var(--text-3); }
  .state.merged { background: var(--accent-bg); color: var(--accent); }
  .state.closed { background: var(--red-bg); color: var(--red); }
  .add { color: var(--green); font-variant-numeric: tabular-nums; }
  .del { color: var(--red); font-variant-numeric: tabular-nums; }
  .link { color: var(--accent); font-size: inherit; padding: 0; }
  .link:hover { text-decoration: underline; }
  .link:disabled { opacity: 0.5; }

  .tabs { display: flex; gap: 2px; border-bottom: 1px solid var(--line); margin-bottom: 12px; }
  .tabs button { color: var(--muted); padding: 4px 10px 8px; font-size: 13px; border-bottom: 2px solid transparent; margin-bottom: -1px; display: inline-flex; gap: 4px; align-items: baseline; }
  .tabs button:hover { color: var(--text-2); }
  .tabs button.on { color: var(--text); border-bottom-color: var(--accent); }
  .n { color: var(--muted); font-size: 12px; }

  .cols { display: grid; grid-template-columns: minmax(0, 1fr) 240px; gap: 18px; align-items: start; }
  .narrow .cols { grid-template-columns: minmax(0, 1fr); }
  .narrow .side { order: -1; }
  .desc { user-select: text; -webkit-user-select: text; font-size: 14px; line-height: 1.6; }
  .latest { display: block; margin-top: 12px; padding-top: 10px; border-top: 1px solid var(--line); color: var(--muted); font-size: 12px; text-align: left; width: 100%; }
  .latest:hover { color: var(--text-2); }
  .latest b { color: var(--text-3); font-weight: 500; }
  .side { display: flex; flex-direction: column; gap: 10px; }
  .side section { background: var(--surface-2); border-radius: var(--r-md); padding: 10px 12px; display: flex; flex-direction: column; gap: 6px; }
  .label { margin-bottom: 0; }
  .line { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text-2); }
  .line.muted { color: var(--muted); }
  .dot { width: 7px; height: 7px; border-radius: 50%; flex: none; background: var(--muted); }
  .line.ok .dot, .dot.pass { background: var(--green); }
  .line.warn .dot, .dot.pending { background: var(--amber); }
  .line.bad .dot, .dot.fail { background: var(--red); }
  .line.merged .dot { background: var(--accent); }
  .dot.skipped { background: none; border: 1.5px solid var(--muted); }
  .merge-row { display: flex; gap: 4px; }
  .merge { flex: 1; background: var(--green-bg); color: var(--green); justify-content: center; }
  .merge:hover:not(:disabled) { background: color-mix(in srgb, var(--green) 22%, transparent); color: var(--green); }
  .merge.armed { background: var(--green); color: var(--bg); }
  .caret { padding: 0 8px; }
  .opt { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-3); cursor: pointer; }
  .side .link { align-self: flex-start; font-size: 12px; }
  .check { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text-2); text-align: left; padding: 2px 0; }
  .check:hover:not(:disabled) { color: var(--text); }
  .check:disabled { cursor: default; }
  .grow { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .chip { background: var(--surface-hover); color: var(--text-3); border-radius: var(--r-sm); padding: 0 8px; font-size: 12px; }
  .none { color: var(--muted); font-size: 13px; margin: 6px 0; overflow-wrap: anywhere; }
  .none.small { font-size: 12px; margin: 0; }

  .talk { display: flex; flex-direction: column; }
  .said { border-top: 1px solid var(--line); padding: 8px 0; }
  .said:first-child { border-top: 0; padding-top: 0; }
  .who { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 8px; color: var(--muted); font-size: 12px; margin-bottom: 3px; }
  .author { color: var(--text-3); font-weight: 500; }
  .verdict.approved { color: var(--green); }
  .verdict.changes_requested { color: var(--amber); }
  .when { color: var(--muted); font-size: 12px; }
  .fix { margin-left: auto; color: var(--accent); font-size: 12px; opacity: 0; }
  .said:hover .fix, .fix:focus-visible { opacity: 1; }
  .body { user-select: text; -webkit-user-select: text; font-size: 14px; line-height: 1.55; }

  .files-bar { display: flex; gap: 12px; align-items: center; color: var(--muted); font-size: 12px; margin-bottom: 6px; }
  .files-bar span { flex: 1; }
  .file.read .file-head { opacity: 0.6; }
  .tick { width: 15px; height: 15px; margin-left: 8px; flex: none; display: grid; place-items: center; border: 1.5px solid var(--line-strong); border-radius: var(--r-xs); color: var(--green); }
  .tick:hover { border-color: var(--text-3); }
  .tick.on { border-color: var(--green); }
  .changed-mark { color: var(--amber); font-size: 11px; margin-right: 6px; }
  .file { border-top: 1px solid var(--line); }
  .file-head { display: flex; align-items: center; gap: 8px; width: 100%; padding: 6px 2px; font-size: 13px; color: var(--text-2); text-align: left; }
  .file-head:hover { color: var(--text); }
  .chev { display: grid; place-items: center; color: var(--muted); transform: rotate(-90deg); transition: transform 0.12s var(--ease); }
  .chev.open { transform: none; }
  pre { margin: 0 0 8px; padding: 6px 0; font-family: var(--mono); font-size: 12px; max-height: 520px; overflow: auto; background: var(--surface); border-radius: var(--r-md); }
  .l { display: block; color: var(--text-3); white-space: pre-wrap; padding: 0 10px; }
  .plus { color: var(--green); background: var(--green-bg); }
  .minus { color: var(--red); background: var(--red-bg); }
  .hunk { color: var(--muted); padding-top: 4px; }

  .commit { display: flex; align-items: baseline; gap: 10px; padding: 6px 0; border-top: 1px solid var(--line); font-size: 13px; color: var(--text-2); }
  .commit:first-child { border-top: 0; }
  .hash { color: var(--muted); font-size: 12px; flex: none; }

  .reply { margin-top: 14px; border: 1px solid var(--line-strong); border-radius: var(--r-lg); padding: 8px 10px; }
  .reply textarea { width: 100%; background: none; border: 0; outline: none; resize: vertical; color: var(--text-2); font: inherit; font-size: 13px; min-height: 38px; }
  .reply-row { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .hint { flex: 1; color: var(--muted); font-size: 12px; }
</style>
