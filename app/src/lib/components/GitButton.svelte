<!-- The thread's Git button, after T3 Code's Git actions. One button that does the next step for where the folder stands (commit, push, open a pull request, or pull), with the words written by a model from the changes and the thread, and a chip for the branch's pull request that opens it in the right panel.
     Used by ThreadView.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { poll } from "../poll";
  import { pop } from "../popover";
  // The thread's Git button, after T3 Code's Git actions. One button that does the next step for
  // where the folder stands (commit, push, open a pull request, or pull), with the words written
  // by a model from the changes and the thread, and a chip for the branch's pull request that
  // opens it in the right panel.
  import { api } from "../api";
  import type { GitState } from "../bindings/GitState";
  import type { Thread } from "../bindings/Thread";
  import { nextStep, type GitStep } from "../github";
  import { contextMenu } from "../menu.svelte";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import Switch from "./Switch.svelte";

  let { thread }: { thread: Thread } = $props();

  let st = $state<GitState | null>(null);
  const load = () => quiet(api.gitState(thread.id).then((s) => (st = s)), "git state");
  $effect(() => {
    void thread.status;
    load();
    const timer = poll(load, 10_000);
    return () => timer();
  });

  const pr = $derived(hub.prs[thread.id]);
  const watched = $derived(hub.watches[thread.id]);
  const step = $derived(st ? nextStep(st, pr) : null);
  const running = $derived(thread.status === "running");

  // The form: what goes in the commit and the pull request, written ahead by a model.
  let open = $state<GitStep | null>(null);
  let message = $state("");
  let title = $state("");
  let body = $state("");
  let draft = $state(false);
  let writing = $state(false);
  let going = $state(false);
  let branchName = $state("");
  let done = $state<string | null>(null);

  async function start(s: GitStep) {
    open = s;
    done = null;
    if (s.pull || (!s.commit && !s.pr)) return go();
    if (message || title) return;
    writing = true;
    try {
      const d = await api.gitDraft(thread.id);
      message ||= d.message;
      title ||= d.title;
      body ||= d.body;
    } catch (e) {
      hub.fail(e);
    } finally {
      writing = false;
    }
  }

  async function go() {
    const s = open;
    if (!s) return;
    going = true;
    try {
      const r = await api.ship(thread.id, { commit: s.commit, push: s.push, pr: s.pr, pull: !!s.pull, message, title, body, draft });
      const parts = [r.commit && `Committed ${r.commit}`, r.pushed && "pushed", r.pr_url && "pull request opened", s.pull && "Pulled"].filter(Boolean);
      done = parts.join(", ") + ".";
      setTimeout(() => (done = null), 6000);
      if (r.pr_url) {
        // The core looks pull requests up every few minutes; show this one now.
        const n = Number(r.pr_url.split("/").pop());
        hub.prs = { ...hub.prs, [thread.id]: { number: n, title, url: r.pr_url, state: draft ? "draft" : "open" } };
      }
      open = null;
      message = title = body = "";
      draft = false;
    } catch (e) {
      hub.fail(e);
    } finally {
      going = false;
      load();
    }
  }

  async function newBranch() {
    const name = branchName.trim();
    if (!name) return;
    try {
      await api.switchBranch(thread.id, name, true);
      branchName = "";
      await load();
      if (st) open = nextStep(st, pr);
    } catch (e) {
      hub.fail(e);
    }
  }

  function showPull() {
    if (!pr) return;
    hub.pull = { thread: thread.id, path: thread.cwd, host: thread.host ?? null, number: pr.number };
  }

  function others(e: MouseEvent) {
    if (!st) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const zoom = Number(document.documentElement.style.zoom) || 1;
    const on = st.branch === st.default;
    contextMenu.show(r.right / zoom - 220, r.bottom / zoom + 4, [
      st.files.length > 0 && { label: "Commit", hint: "only", run: () => start({ label: "Commit", commit: true, push: false, pr: false }) },
      st.files.length > 0 && st.github && { label: "Commit & push", run: () => start({ label: "Commit & push", commit: true, push: true, pr: false, toDefault: on }) },
      st.github && !st.files.length && (st.ahead > 0 || !st.upstream) && { label: "Push", run: () => start({ label: "Push", commit: false, push: true, pr: false, toDefault: on }) },
      st.github && !on && !pr && { label: "Create pull request", run: () => start({ label: "Create PR", commit: false, push: !st!.upstream || st!.ahead > 0, pr: true }) },
      st.upstream && { label: "Pull", hint: st.behind ? `${st.behind} new` : undefined, run: () => start({ label: "Pull", commit: false, push: false, pr: false, pull: true }) },
      null,
      pr && { label: `Open pull request #${pr.number}`, run: showPull },
      pr && !watched && pr.state !== "merged" && pr.state !== "closed" && {
        label: "Watch it for this thread",
        hint: "wakes the agent",
        run: () => api.watch(thread.id, pr.number).then((n) => (hub.watches = { ...hub.watches, [thread.id]: n })).catch((err) => hub.fail(err)),
      },
      !!watched && { label: "Stop watching", run: () => api.unwatch(thread.id).then(() => { const { [thread.id]: _, ...rest } = hub.watches; hub.watches = rest; }).catch((err) => hub.fail(err)) },
    ]);
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") open = null;
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && !writing && !going) go();
  }
</script>

{#if st?.branch}
  <div class="git">
    {#if pr}
      <button class="pr {pr.state}" onclick={showPull} data-tip="{pr.title}{watched ? ' · Hub watches it for this thread' : ''}">
        <Icon name="pr" size={12} />#{pr.number}{#if watched}<Icon name="eye" size={11} />{/if}
      </button>
    {/if}
    {#if done}
      <span class="done"><Icon name="check" size={12} />{done}</span>
    {:else}
      <div class="split">
        <button class="go" disabled={!step || going || running} onclick={() => step && start(step)} data-tip={running ? "Waits until the turn ends" : step ? step.label : "Nothing to commit or push"}>
          <Icon name="branch" size={12} /><span class="text">{step?.label ?? "Up to date"}</span>
        </button>
        <button class="caret" data-tip="Other Git actions" onclick={others}><Icon name="down" size={11} /></button>
      </div>
    {/if}
  </div>
{/if}

{#if open && !open.pull && (open.commit || open.pr)}
  <div role="presentation" class="form" transition:pop|global onkeydown={key}>
    {#if open.toDefault}
      <div class="warn">
        This goes straight to <span class="mono">{st?.default}</span>.
        <span class="new-branch"><input bind:value={branchName} placeholder="or a new branch name" spellcheck="false" onkeydown={(e) => e.key === "Enter" && newBranch()} /><button class="link" disabled={!branchName.trim()} onclick={newBranch}>Make branch</button></span>
      </div>
    {/if}
    {#if open.commit}
      <label class="label" for="git-msg">Commit message · {st?.files.length} {st?.files.length === 1 ? "file" : "files"}</label>
      <textarea id="git-msg" bind:value={message} rows="4" placeholder={writing ? "Writing from the changes and the thread…" : "What changed and why"}></textarea>
    {/if}
    {#if open.pr}
      <label class="label" for="git-title">Pull request</label>
      <input id="git-title" bind:value={title} placeholder={writing ? "Writing…" : "Title"} />
      <textarea bind:value={body} rows="5" placeholder={writing ? "" : "What it changes, why, and how it was checked"}></textarea>
      <label class="opt"><Switch label="Open as a draft" on={draft} onchange={(on) => (draft = on)} /> Open as a draft</label>
    {/if}
    <div class="row">
      <span class="hint">{writing ? "Writing…" : "Ctrl Enter"}</span>
      <button class="btn" onclick={() => (open = null)}>Cancel</button>
      <button class="btn primary" disabled={writing || going || (open.commit && !message.trim()) || (open.pr && !title.trim())} onclick={go}>{going ? "Working…" : open.label}</button>
    </div>
  </div>
{/if}

<style>
  .git { display: flex; align-items: center; gap: 6px; flex: none; }
  .pr { display: inline-flex; align-items: center; gap: 4px; height: 26px; padding: 0 8px; border-radius: var(--r-md); font-size: 12px; font-family: var(--mono); background: var(--surface-2); color: var(--text-3); }
  .pr:hover { color: var(--text); background: var(--surface-hover); }
  .pr.open { color: var(--green); }
  .pr.merged { color: var(--accent); }
  .pr.closed { color: var(--red); }
  .split { display: flex; height: 26px; border-radius: var(--r-md); overflow: hidden; background: var(--surface-2); }
  .go { display: inline-flex; align-items: center; gap: 6px; padding: 0 10px; font-size: 13px; color: var(--text-2); white-space: nowrap; }
  .go:hover:not(:disabled), .caret:hover { background: var(--surface-hover); color: var(--text); }
  .go:disabled { color: var(--muted); cursor: default; }
  /* A narrow thread (the panel beside it is open) keeps the icon and drops the words. */
  @container thread-head (max-width: 760px) { .go .text { display: none; } .go { padding: 0 8px; } }
  .caret { display: grid; place-items: center; width: 22px; color: var(--muted); border-left: 1px solid var(--line); }
  .done { display: inline-flex; align-items: center; gap: 4px; color: var(--green); font-size: 12px; white-space: nowrap; }
  .form { position: absolute; right: 16px; top: calc(100% + 4px); width: min(460px, calc(100vw - 32px)); z-index: 30; background: var(--surface); border: 1px solid var(--line-strong); border-radius: var(--r-lg); padding: 12px 14px; box-shadow: 0 12px 32px rgba(0, 0, 0, 0.4); display: flex; flex-direction: column; gap: 6px; }
  .form textarea, .form input:not([type]) { background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 6px 8px; font: inherit; font-size: 13px; color: var(--text-2); outline: none; resize: vertical; width: 100%; }
  .form textarea:focus, .form input:not([type]):focus { border-color: var(--accent); }
  .label { margin-top: 4px; }
  .opt { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text-3); }
  .row { display: flex; align-items: center; gap: 6px; margin-top: 4px; }
  .hint { flex: 1; color: var(--muted); font-size: 12px; }
  .warn { background: var(--amber-bg); color: var(--amber); border-radius: var(--r-md); padding: 8px 10px; font-size: 13px; display: flex; flex-direction: column; gap: 6px; }
  .new-branch { display: flex; gap: 6px; align-items: center; }
  .new-branch input { flex: 1; min-width: 0; background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-sm); padding: 3px 8px; font: inherit; font-size: 13px; color: var(--text-2); outline: none; }
  .link { color: var(--accent); font-size: 13px; padding: 0; white-space: nowrap; }
  .link:disabled { opacity: 0.5; }
</style>
