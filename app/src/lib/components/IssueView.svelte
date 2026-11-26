<!-- One issue: what it says, its comments, a box to answer in, close or reopen, and start a thread that works on it.
     Used by GithubTab.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // One issue: what it says, its comments, a box to answer in, close or reopen, and start a
  // thread that works on it.
  import { api } from "../api";
  import type { IssueDetail } from "../bindings/IssueDetail";
  import { ago } from "../format";
  import { menu } from "../menu.svelte";
  import { linkMenu } from "../menus.svelte";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import Markdown from "./Markdown.svelte";

  let {
    path,
    host = null,
    number,
    onthread,
    onchange,
  }: { path: string; host?: string | null; number: number; onthread?: (text: string) => void; onchange?: () => void } = $props();

  let d = $state<IssueDetail | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let reply = $state("");

  async function load() {
    try {
      const got = await api.issue(path, number, host);
      if (got.issue.number === number) d = got;
      error = null;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  $effect(() => {
    void number;
    d = null;
    load();
  });

  async function act(action: string, body?: string) {
    busy = true;
    try {
      await api.issueAction({ path, number, action, title: null, body: body ?? null }, host);
      await load();
      onchange?.();
      return true;
    } catch (e) {
      hub.fail(e);
      return false;
    } finally {
      busy = false;
    }
  }
  async function comment() {
    if (await act("comment", reply)) reply = "";
  }
  function start() {
    if (!d) return;
    const i = d.issue;
    onthread?.(
      `Work on issue #${i.number} "${i.title}" (${i.url}).\n\n${d.body.trim() ? `It says:\n\n> ${d.body.trim().replace(/\n/g, "\n> ")}\n\n` : ""}Read its comments with \`gh issue view ${i.number} --comments\`. Work on a new branch, open a pull request that says \`Fixes #${i.number}\`, and tell me when it is ready.`,
    );
  }
</script>

<div class="issue">
  {#if error && !d}
    <p class="none">{error}</p>
  {:else if !d}
    <p class="none">Loading #{number}…</p>
  {:else}
    {@const i = d.issue}
    <header>
      <h2 use:menu={() => linkMenu(i.url)}>{i.title} <span class="num">#{i.number}</span></h2>
      <div class="meta">
        <span class="state {i.state}">{i.state === "open" ? "Open" : "Closed"}</span>
        <span>by {i.author} · {ago(d.created_at)} ago</span>
        {#each i.labels as l (l)}<span class="chip">{l}</span>{/each}
        <span class="grow"></span>
        {#if onthread && i.state === "open"}<button class="btn primary" onclick={start}><Icon name="play" size={12} /> Start a thread</button>{/if}
        <button class="btn" onclick={() => openUrl(i.url)}>Open on GitHub</button>
        {#if i.state === "open"}
          <button class="btn danger" disabled={busy} onclick={() => act("close")}>Close</button>
        {:else}
          <button class="btn" disabled={busy} onclick={() => act("reopen")}>Reopen</button>
        {/if}
      </div>
    </header>
    {#if d.body.trim()}<div class="body"><Markdown text={d.body} /></div>{:else}<Quiet text="No description" />{/if}
    {#each d.comments as c, k (k)}
      <div class="said">
        <div class="who"><span class="author">{c.author}</span><span>{ago(c.at)} ago</span></div>
        <div class="body"><Markdown text={c.body} /></div>
      </div>
    {/each}
    <div class="reply">
      <textarea bind:value={reply} placeholder="Comment…" rows="2" onkeydown={(e) => e.key === "Enter" && (e.ctrlKey || e.metaKey) && reply.trim() && comment()}></textarea>
      <div class="reply-row"><span class="hint">Ctrl Enter comments</span><button class="btn" disabled={!reply.trim() || busy} onclick={comment}>Comment</button></div>
    </div>
  {/if}
</div>

<style>
  header { margin-bottom: 12px; }
  h2 { color: var(--text); font-size: 16px; font-weight: 500; margin: 0; line-height: 1.35; }
  .num { color: var(--muted); font-weight: 400; }
  .meta { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 10px; color: var(--muted); font-size: 12px; margin-top: 8px; }
  .grow { flex: 1; }
  .state { border-radius: var(--r-sm); padding: 0 8px; font-size: 12px; }
  .state.open { background: var(--green-bg); color: var(--green); }
  .state.closed { background: var(--accent-bg); color: var(--accent); }
  .chip { background: var(--surface-hover); color: var(--text-3); border-radius: var(--r-sm); padding: 0 8px; font-size: 12px; }
  .body { user-select: text; -webkit-user-select: text; font-size: 14px; line-height: 1.6; }
  .said { border-top: 1px solid var(--line); padding: 8px 0; margin-top: 10px; }
  .who { display: flex; gap: 8px; color: var(--muted); font-size: 12px; margin-bottom: 3px; }
  .author { color: var(--text-3); font-weight: 500; }
  .none { color: var(--muted); font-size: 13px; }
  .reply { margin-top: 14px; border: 1px solid var(--line-strong); border-radius: var(--r-lg); padding: 8px 10px; }
  .reply textarea { width: 100%; background: none; border: 0; outline: none; resize: vertical; color: var(--text-2); font: inherit; font-size: 13px; }
  .reply-row { display: flex; align-items: center; gap: 6px; }
  .hint { flex: 1; color: var(--muted); font-size: 12px; }
</style>
