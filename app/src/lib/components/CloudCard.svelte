<!-- A thread that runs in the cloud: where it stands, where its work will land, and what you can do with it. Hub shows status, branch and pull request; the conversation itself lives on the service's own page, which "Open" goes to. A follow-up is the box below, as in any thread.
     Used by ThreadView.svelte. -->
<script lang="ts">
  // A thread that runs in the cloud: where it stands, where its work will land, and what you can
  // do with it. Hub shows status, branch and pull request; the conversation itself lives on the
  // service's own page, which "Open" goes to. A follow-up is the box below, as in any thread.
  import { api } from "../api";
  import type { Thread } from "../bindings/Thread";
  import { cloudHost, cloudLabel, cloudStateLabel, cloudStep, type CloudRun } from "../cloud.svelte";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let { thread, cloud }: { thread: Thread; cloud: CloudRun } = $props();

  const step = $derived(cloudStep(cloud));
  const live = $derived(cloud.state === "starting" || cloud.state === "working" || cloud.state === "waiting");
  const steps = ["Started", "Working", "Branch pushed", "PR opened"];

</script>

<section class="cloud" aria-label="Cloud thread">
  <div class="steps" data-tip={cloudStateLabel(cloud)}>
    {#each steps as s, i (s)}
      <span class="step" class:done={i + 1 < step || (i + 1 === step && !live)} class:cur={i + 1 === step && live}>{s}</span>{#if i < steps.length - 1}<span class="bar"></span>{/if}
    {/each}
  </div>
  <div class="grid">
    <span class="k">Runs on</span><span>{cloudHost(cloud.provider)} <span class="muted">· your {cloud.provider === "claude" ? "Claude" : "ChatGPT"} subscription</span></span>
    {#if cloud.repo}<span class="k">Repo</span><span>{cloud.repo}</span>{/if}
    {#if cloud.branch}<span class="k">Branch</span><span class="mono">{cloud.branch}</span>{/if}
    {#if cloud.last}<span class="k">Last</span><span class="muted">{cloud.last}</span>{/if}
    {#if cloud.note}<span class="k">Note</span><span class="note">{cloud.note}</span>{/if}
  </div>
  <div class="actions">
    {#if cloud.url}<button class="go" onclick={() => openUrl(cloud.url!)} data-tip="The conversation lives on the service's own page">Open on {cloud.provider === "claude" ? "claude.ai" : "chatgpt.com"}</button>{/if}
    {#if live}<button onclick={() => api.cancel(thread.id).catch((e) => hub.fail(e))}><Icon name="stop" size={11} /> Stop</button>{/if}
  </div>
  {#if cloud.pr}
    <button class="pr" onclick={() => openUrl(cloud.pr!.url)}>
      <Icon name="branch" size={13} />
      <span class="grow">Pull request #{cloud.pr.number}{cloud.pr.title ? ` · ${cloud.pr.title}` : ""}</span>
      {#if cloud.pr.additions != null}<span class="add">+{cloud.pr.additions}</span><span class="del">−{cloud.pr.deletions ?? 0}</span>{/if}
    </button>
  {/if}
  <p class="why">{cloudLabel(cloud.provider)} works on a copy of {cloud.repo ?? "the repository"} from GitHub and ends in a branch and a pull request. When the subscription is full it stops; Hub never switches to an API key.</p>
</section>

<style>
  .cloud { margin: 14px 0 10px; padding: 12px 14px; background: var(--surface); border-radius: var(--r-lg); display: flex; flex-direction: column; gap: 10px; font-size: 13px; color: var(--text-2); }
  .steps { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--muted); flex-wrap: wrap; }
  .step.done { color: var(--green); }
  .step.cur { color: var(--text); }
  .step.cur::before { content: ""; display: inline-block; width: 6px; height: 6px; border-radius: 50%; border: 1.5px solid var(--green); margin-right: 6px; vertical-align: 1px; }
  .bar { width: 16px; height: 1px; background: var(--line-strong); }
  .grid { display: grid; grid-template-columns: 70px 1fr; gap: 4px 10px; align-items: baseline; min-width: 0; }
  .k { color: var(--muted); font-size: 12px; }
  .muted { color: var(--muted); }
  .mono { font-family: var(--mono); font-size: 12px; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; }
  .note { color: var(--amber); }
  .actions { display: flex; gap: 8px; flex-wrap: wrap; }
  .actions button { height: 26px; padding: 0 11px; border-radius: var(--r-sm); background: var(--surface-hover); color: var(--text-2); font-size: 12px; display: inline-flex; align-items: center; gap: 6px; font-family: var(--font, inherit); }
  .actions button:hover:not(:disabled) { color: var(--text); }
  .actions button.go { background: var(--accent-bg); color: var(--accent); }
  .pr { display: flex; align-items: center; gap: 8px; width: 100%; padding: 8px 10px; border-radius: var(--r-md); background: var(--surface-2); color: var(--text-2); text-align: left; font-size: 13px; font-family: var(--font, inherit); }
  .pr:hover { color: var(--text); }
  .grow { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .add { color: var(--green); font-family: var(--mono); font-size: 12px; }
  .del { color: var(--red); font-family: var(--mono); font-size: 12px; }
  .why { margin: 0; color: var(--muted); font-size: 12px; line-height: 1.5; }
</style>
