<!-- Design mode in the transcript (`design.rs`): one line when it is turned on or off, one when a turn changed code anyway, and the goldfish review as one line that opens to its four checks.
     Used by Transcript.svelte. -->
<script lang="ts">
  // Design mode in the transcript (`design.rs`): one line when it is turned on or off, one when a
  // turn changed code anyway, and the goldfish review as one line that opens to its four checks.
  import { menu } from "../menu.svelte";
  import { messageMenu } from "../menus.svelte";
  import type { DesignCheck, Item } from "../transcript";
  import Icon from "./Icon.svelte";
  import IconSwap from "./IconSwap.svelte";
  import Markdown from "./Markdown.svelte";

  type DesignItem = Extract<Item, { kind: "design" | "design_review" | "design_strayed" }>;
  let { item }: { item: DesignItem; threadId: string } = $props();

  // A check that found nothing reads as fine; one that found something as a warning.
  const FINE: Record<string, string> = { explain: "clear", gaps: "no gaps", fit: "fits", ready: "ready to build" };
  const toneOf = (c: DesignCheck) => {
    if (c.state === "failed") return "bad";
    if (c.state !== "done") return "";
    const v = (c.verdict ?? "").toLowerCase();
    return v.startsWith(FINE[c.name] ?? "\u0000") && !v.startsWith("not ") ? "ok" : "warn";
  };
  const body = (c: DesignCheck) => {
    const lines = c.text?.trim().split("\n") ?? [];
    return (c.verdict ? lines.slice(1).join("\n").trim() : c.text?.trim()) ?? "";
  };
  const done = $derived(item.kind === "design_review" ? item.checks.filter((c) => c.state !== "running").length : 0);
  const tone = $derived(item.kind === "design_review" ? (item.state === "failed" ? "bad" : item.ready ? (item.sent ? "warn" : "ok") : "warn") : "");
  let open = $state(false);
  let openCheck = $state<number | null>(null);
</script>

{#if item.kind === "design"}
  <div class="note">
    <Icon name="design" size={13} />
    {#if item.on}
      <span>{item.by === "start" ? "Started in design mode" : "Design mode on"}<span class="sep">·</span>the agent plans in a document under <span class="mono">docs/design/</span> and writes no code</span>
    {:else}
      <span>Design mode off<span class="sep">·</span>the agent may build now</span>
    {/if}
  </div>
{:else if item.kind === "design_strayed"}
  <div class="note">
    <span class="warn"><Icon name="alert" size={13} /></span>
    <span>
      <span class="warn">Changed outside the design document</span> in design mode:
      <span class="mono">{item.files.map((f) => f.split("/").slice(-2).join("/")).join(", ")}</span>. The engine edited without asking, so Hub could not stop it; set what it may do without asking to a mode that asks.
    </span>
  </div>
{:else}
  <div class="review">
    <button class="line" onclick={() => (open = !open)} aria-expanded={open}>
      {#if item.state === "running"}
        <span class="pulse"></span>
        <span class="warn">{item.by === "agent" ? "The agent asked for a design review" : "Design review"}<span class="sep">·</span>{done} of {item.checks.length} fresh readers done with <span class="mono">{item.doc}</span>…</span>
      {:else if item.state === "failed"}
        <Icon name="design" size={13} /><span class="bad">Design review failed</span>
      {:else}
        <span class="stamp {tone}"><IconSwap name={item.ready ? "check" : "design"} size={12} /></span>
        <span>
          Design reviewed by {item.checks.length} fresh readers<span class="sep">·</span><span class={tone}>{item.verdict}</span>
          {#if item.sent}<span class="sep">·</span>{#if item.fixed}<span class="ok">the agent went through it</span>{:else}<span class="warn">the agent is going through it…</span>{/if}{/if}
        </span>
      {/if}
      <span class="chev" class:open><Icon name="chevron" size={11} /></span>
    </button>
    {#if open}
      <div class="body">
        {#if item.error}<p class="bad">{item.error}</p>{/if}
        <p class="what">They never saw this conversation, only <span class="mono">{item.doc}</span>.</p>
        {#each item.checks as c, i (i)}
          {@const t = toneOf(c)}
          <button class="part" onclick={() => (openCheck = openCheck === i ? null : i)} disabled={c.state === "running"}>
            <span class="mark {t}">
              {#if c.state === "running"}<span class="dot"></span>{:else if t === "ok"}<Icon name="check" size={11} />{:else if t === "bad"}!{:else}<Icon name="eye" size={11} />{/if}
            </span>
            <span class="ptitle">{c.title}</span>
            <span class="pmeta">{#if c.model}{c.model}{/if}{#if c.verdict}{c.model ? " · " : ""}<span class={t}>{c.verdict}</span>{:else if c.state === "running"}{c.model ? " · " : ""}reading…{/if}</span>
          </button>
          {#if openCheck === i}
            <div class="ptext" use:menu={() => messageMenu(c.text ?? c.error ?? "")}>
              {#if c.error}<p class="bad">{c.error}</p>{:else if body(c)}<Markdown text={body(c)} />{/if}
            </div>
          {/if}
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .note { display: flex; align-items: flex-start; gap: 8px; margin: 4px 0 8px; font-size: 13px; color: var(--muted); }
  .note :global(svg) { flex: none; margin-top: 3px; }
  .mono { font-family: var(--mono); font-size: 12px; color: var(--text-3); }
  .review { margin: 4px 0 8px; font-size: 13px; }
  .line { display: inline-flex; align-items: center; gap: 8px; max-width: 100%; color: var(--muted); padding: 3px 6px; margin-left: -6px; border-radius: var(--r-sm); text-align: left; }
  .line:hover { color: var(--text-3); background: var(--surface); }
  .line > span:not(.stamp):not(.chev):not(.pulse) { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
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
  .what { margin: 2px 0 4px; color: var(--muted); font-size: 13px; }
  .part { display: flex; align-items: center; gap: 8px; width: 100%; padding: 3px 6px; margin-left: -6px; border-radius: var(--r-sm); text-align: left; color: var(--text-3); font-size: 13px; }
  .part:hover:not(:disabled) { background: var(--surface); color: var(--text-2); }
  .part:disabled { cursor: default; }
  .mark { width: 14px; height: 14px; display: grid; place-items: center; flex: none; color: var(--muted); font-size: 11px; }
  .mark.ok { color: var(--green); }
  .mark.warn { color: var(--amber); }
  .mark.bad { color: var(--red); }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--amber); }
  .ptitle { color: var(--text-2); white-space: nowrap; }
  .pmeta { color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ptext { margin: 2px 0 8px 22px; line-height: 1.6; font-size: 14px; }
</style>
