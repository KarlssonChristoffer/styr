<!-- A turn that failed, as a card: the engine's words, where the failure stands in Problems, and "Fix now", which starts a fix thread on it in the background. When that thread's first turn ends, the card says what it found and offers to send the failed message again.
     Used by Transcript.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  // A turn that failed, as a card: the engine's words, where the failure stands in Problems, and
  // "Fix now", which starts a fix thread on it in the background. When that thread's first turn
  // ends, the card says what it found and offers to send the failed message again.
  import { api } from "../api";
  import type { Problem } from "../bindings/Problem";
  import { hub } from "../state.svelte";
  import type { ErrorItem } from "../transcript";
  import Icon from "./Icon.svelte";

  let {
    item,
    threadId,
    host = null,
    live = false,
    retry,
  }: {
    item: ErrorItem;
    threadId: string;
    host?: string | null;
    live?: boolean;
    /** Sends the message whose turn failed again; missing when it is not known. */
    retry?: () => void;
  } = $props();

  let problem = $state<Problem | null>(null);
  let busy = $state(false);
  const report = $derived(item.report);

  // Where the problem stands now, read again when problems change.
  $effect(() => {
    const id = report?.problem;
    void hub.problemsTick;
    if (id == null) return;
    const t = setTimeout(() => quiet(api.problem(id, host).then((d) => (problem = d.problem)), "problem"), 300);
    return () => clearTimeout(t);
  });

  // A thread working on the problem right now, from here or from Guard.
  const onIt = $derived(problem?.thread ? hub.threads.find((t) => t.id === problem!.thread && (t.status === "running" || t.status === "waiting_on_you")) : undefined);
  const where = $derived.by(() => {
    if (!problem) return "";
    if (onIt) return "a fix thread is on it";
    if (problem.status === "resolved") return problem.resolved_by ? `marked fixed by ${problem.resolved_by}` : "marked fixed";
    if (problem.status === "archived") return "archived";
    if (problem.count > 1) return `seen ${problem.count} times`;
    return "open";
  });

  async function fix() {
    busy = true;
    try {
      await api.fixNow(threadId, item.seq, host);
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = false;
    }
  }

  function openProblem() {
    if (!report) return;
    hub.problemPick = { id: report.problem, host };
    hub.show({ view: "problems" });
  }

  const unreleased = $derived(report?.pr && report.pr.state !== "merged");
</script>

<div class="card" class:done={report?.state === "fixed"}>
  <div class="head">
    <Icon name="alert" size={13} />
    <span class="msg">{item.message}{#if item.times > 1}<span class="times">{" · "}{item.times} times in a row</span>{/if}</span>
  </div>
  {#if item.detail}<span class="detail mono">{item.detail}</span>{/if}

  {#if report}
    <div class="state">
      {#if report.state === "sent"}
        <span class="dot"></span>
        <span>Sent to <button class="link inline" onclick={openProblem}>Problems #{report.problem}</button>{where ? ` · ${where}` : ""}</span>
        {#if onIt}
          <button class="link" onclick={() => hub.open(onIt.id)}>Open it</button>
        {:else}
          <button class="btn" disabled={busy} onclick={fix} data-tip="Start a thread in the background that finds the cause and fixes it, then reports back here">
            <Icon name="spark" size={12} />{busy ? "Starting…" : "Fix now"}
          </button>
        {/if}
      {:else if report.state === "fixing"}
        <span class="dot work"></span>
        <span>Fixing in the background · Problems #{report.problem}</span>
        {#if report.fix}<button class="link" onclick={() => hub.open(report.fix!)}>Open the fix thread</button>{/if}
      {:else if report.state === "fixed"}
        <span class="dot ok"></span>
        <span>
          Fixed{report.pr ? ` in PR #${report.pr.number}` : ""}.
          {#if unreleased}It reaches this machine once the PR is merged and you release it under Settings > Updates; a fix outside the code works now.{:else}Try it again.{/if}
        </span>
      {:else}
        <span class="dot bad"></span>
        <span>The fix thread {report.state === "stopped" ? "was stopped" : "failed"} · Problems #{report.problem}</span>
        <button class="btn" disabled={busy} onclick={fix}><Icon name="spark" size={12} />{busy ? "Starting…" : "Try fixing again"}</button>
      {/if}
    </div>
    {#if report.text && report.state !== "fixing"}
      <p class="said">{report.text}</p>
    {/if}
    {#if report.state === "fixed" || report.state === "stopped" || report.state === "failed"}
      <div class="acts">
        {#if retry && !live}<button class="btn" onclick={retry}><Icon name="restart" size={12} />Try again</button>{/if}
        {#if report.fix}<button class="link" onclick={() => hub.open(report.fix!)}>Open the fix thread</button>{/if}
        {#if report.pr}<a class="link" href={report.pr.url} target="_blank" rel="noreferrer">PR #{report.pr.number}</a>{/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .card { margin: 8px 0; padding: 8px 12px; border: 1px solid var(--line-strong); border-left: 2px solid var(--red); border-radius: var(--r-md); font-size: 13px; color: var(--text-3); }
  .card.done { border-left-color: var(--green); }
  .head { display: flex; align-items: flex-start; gap: 8px; color: var(--red); }
  .card.done .head { color: var(--text-3); }
  .head :global(svg) { flex: none; margin-top: 2px; }
  .msg { user-select: text; -webkit-user-select: text; min-width: 0; word-break: break-word; }
  .times { color: var(--muted); }
  .detail { display: block; margin: 3px 0 0 21px; color: var(--muted); font-size: 12px; white-space: pre-wrap; word-break: break-word; max-height: 120px; overflow: auto; }
  .state { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; margin-top: 6px; padding-left: 21px; }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--muted); flex: none; }
  .dot.work { background: var(--accent); }
  .dot.ok { background: var(--green); }
  .dot.bad { background: var(--red); }
  .said { margin: 6px 0 0 21px; color: var(--text-2); white-space: pre-wrap; word-break: break-word; max-height: 160px; overflow: auto; }
  .acts { display: flex; align-items: center; gap: 8px; margin-top: 6px; padding-left: 21px; }
  .btn { height: 24px; padding: 0 8px; font-size: 12px; display: inline-flex; align-items: center; gap: 6px; }
  .link { color: var(--accent); font-size: 13px; padding: 1px 6px; border-radius: var(--r-sm); }
  .link:hover { background: var(--accent-bg); }
  .link.inline { padding: 0 2px; font-size: inherit; }
</style>
