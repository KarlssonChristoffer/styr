<!-- A second opinion, as one line: who asked, who reviewed, the verdict, and whether the findings went back to the agent. It opens to the parts the changes were cut into, each stamped.
     Used by Transcript.svelte. -->
<script lang="ts">
  // A second opinion, as one line: who asked, who reviewed, the verdict, and whether the findings
  // went back to the agent. It opens to the parts the changes were cut into, each stamped.
  import { menu } from "../menu.svelte";
  import { messageMenu } from "../menus.svelte";
  import { hub } from "../state.svelte";
  import type { ReviewItem, ReviewPart } from "../transcript";
  import Icon from "./Icon.svelte";
  import IconSwap from "./IconSwap.svelte";
  import Markdown from "./Markdown.svelte";

  let { item, threadId }: { item: ReviewItem; threadId: string } = $props();

  const model = $derived(item.model ? item.model[0].toUpperCase() + item.model.slice(1) : "another model");
  const toneOf = (v?: string) => (!v ? "warn" : /^(looks good|ser bra ut|lgtm)/i.test(v) ? "ok" : /needs|behöver/i.test(v) ? "bad" : "warn");
  const verdict = $derived.by(() => {
    const first = item.text?.trim().split("\n")[0]?.replace(/[*_#]/g, "").trim() ?? "";
    return first.length <= 60 ? first.replace(/[.:]$/, "") : "";
  });
  const tone = $derived(toneOf(verdict));
  const reviewed = $derived(item.parts.filter((p) => p.state !== "skipped"));
  const now = $derived(item.parts.findIndex((p) => p.state === "running"));
  const many = $derived(item.parts.length > 1);
  // The body of a part's review, without its verdict line.
  const body = (p: ReviewPart) => {
    const lines = p.text?.trim().split("\n") ?? [];
    return (p.verdict ? lines.slice(1).join("\n").trim() : p.text?.trim()) ?? "";
  };
  let open = $state(false);
  let openPart = $state<number | null>(null);
  let sent = $state(false);

  async function sendBack() {
    sent = true;
    const text = `A second opinion from another model reviewed your changes. Go through its findings: fix what is right, and say briefly why for anything you leave as it is.\n\n${item.text}`;
    await hub.send(threadId, text).catch(() => (sent = false));
  }
</script>

<div class="review">
  <button class="line" onclick={() => (open = !open)} aria-expanded={open}>
    {#if item.state === "running"}
      <span class="pulse"></span>
      <span class="warn">
        {#if item.by === "agent"}The agent asked for a review{#if item.reason}: {item.reason}{/if}<span class="sep">·</span>{/if}
        {!item.parts.length
          ? `${model} is looking at ${item.files.length} ${item.files.length === 1 ? "file" : "files"}…`
          : many && now >= 0
            ? `${model} is reviewing part ${reviewed.findIndex((p) => p === item.parts[now]) + 1} of ${reviewed.length}…`
            : `${model} is reviewing…`}
      </span>
    {:else if item.state === "failed"}
      <Icon name="eye" size={13} /><span class="bad">Review by {model} failed</span>
    {:else}
      <span class="stamp {tone}"><IconSwap name={tone === "ok" ? "check" : "eye"} size={12} /></span>
      <span>
        {item.by === "agent" ? "The agent had" : "Reviewed by"} {item.by === "agent" ? `${model} review` : model}{#if item.by === "agent" && item.reason}: {item.reason}{/if}
        {#if verdict}<span class="sep">·</span><span class={tone}>{verdict}</span>{/if}
        {#if many}<span class="sep">·</span>{item.parts.length} parts{/if}
        {#if item.sent}<span class="sep">·</span>{#if item.fixed}<span class="ok">fixed by the agent</span>{:else}<span class="warn">agent is fixing…</span>{/if}{/if}
      </span>
    {/if}
    <span class="chev" class:open><Icon name="chevron" size={11} /></span>
  </button>
  {#if open}
    <div class="body">
      {#if item.state === "failed"}<p class="bad">{item.error}</p>{/if}
      {#each item.parts as p, i (i)}
        {@const t = p.state === "done" ? toneOf(p.verdict) : ""}
        <button class="part" onclick={() => (openPart = openPart === i ? null : i)} disabled={p.state !== "done" && p.state !== "failed"} data-tip={p.files.join("\n")}>
          <span class="mark {p.state} {t}">
            {#if p.state === "running"}<span class="dot running"></span>
            {:else if p.state === "done"}<IconSwap name={t === "ok" ? "check" : "eye"} size={11} />
            {:else if p.state === "skipped"}–{:else if p.state === "failed"}!{:else}·{/if}
          </span>
          <span class="ptitle">{p.title}</span>
          <span class="pmeta">{p.files.length} {p.files.length === 1 ? "file" : "files"}{#if p.state === "skipped"} · skipped{p.why ? `: ${p.why}` : ""}{:else if p.verdict} · <span class={t}>{p.verdict}</span>{/if}</span>
        </button>
        {#if openPart === i}
          <div class="ptext" use:menu={() => messageMenu(p.text ?? p.error ?? "")}>
            {#if p.error}<p class="bad">{p.error}</p>{:else if body(p)}<Markdown text={body(p)} />{/if}
          </div>
        {/if}
      {/each}
      {#if item.state === "done" && !item.sent && tone !== "ok"}
        <button class="link" disabled={sent} onclick={sendBack}>{sent ? "Sent to the agent" : "Send to the agent"}</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .review { margin: 4px 0 8px; font-size: 13px; }
  .line { display: inline-flex; align-items: center; gap: 8px; max-width: 100%; color: var(--muted); padding: 3px 6px; margin-left: -6px; border-radius: var(--r-sm); text-align: left; }
  .line:hover { color: var(--text-3); background: var(--surface); }
  .line > span:not(.stamp):not(.chev):not(.dot) { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .stamp { display: grid; place-items: center; width: 16px; height: 16px; border-radius: 50%; flex: none; }
  .stamp.ok { color: var(--green); background: var(--green-bg); }
  .stamp.warn { color: var(--amber); background: var(--amber-bg); }
  .stamp.bad { color: var(--red); background: var(--red-bg); }
  .sep { margin: 0 6px; opacity: 0.6; }
  .ok { color: var(--green); }
  .warn { color: var(--amber); }
  .bad { color: var(--red); }
  .pulse { width: 7px; height: 7px; border-radius: 50%; background: var(--amber); flex: none; }
  .chev { display: grid; transition: transform 0.12s var(--ease); opacity: 0.7; flex: none; }
  .chev.open { transform: rotate(90deg); }
  .body { border-left: 1px solid var(--line-strong); margin: 4px 0 4px 8px; padding: 2px 0 2px 12px; }
  .part { display: flex; align-items: center; gap: 8px; width: 100%; padding: 3px 6px; margin-left: -6px; border-radius: var(--r-sm); text-align: left; color: var(--text-3); font-size: 13px; }
  .part:hover:not(:disabled) { background: var(--surface); color: var(--text-2); }
  .part:disabled { cursor: default; }
  .mark { width: 14px; height: 14px; display: grid; place-items: center; flex: none; color: var(--muted); font-size: 11px; }
  .mark.ok { color: var(--green); }
  .mark.warn { color: var(--amber); }
  .mark.bad, .mark.failed { color: var(--red); }
  .ptitle { color: var(--text-2); white-space: nowrap; }
  .pmeta { color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ptext { margin: 2px 0 8px 22px; line-height: 1.6; font-size: 14px; }
  .link { color: var(--accent); font-size: 13px; padding: 1px 6px; margin: 6px 0 0 -6px; border-radius: var(--r-sm); }
  .link:hover { background: var(--accent-bg); }
</style>
