<!-- The card for a pull request named in a chat: its state, repo and number, how long ago, title, author, size and checks. On a desktop it floats by the reference after a short rest and stays while the pointer is over it; where there is no hover (a phone) a tap opens it as a sheet from the bottom, with the ways in. Facts load through `prCard.svelte.ts`; what Hub already knows of the pull request fills the card meanwhile, and stays when GitHub cannot be reached.
     Used by Markdown.svelte. -->
<script lang="ts">
  import { fade } from "svelte/transition";
  import { slide } from "../pop";
  // The card for a pull request named in a chat: its state, repo and number, how long ago, title,
  // author, size and checks. On a desktop it floats by the reference after a short rest and stays
  // while the pointer is over it; where there is no hover (a phone) a tap opens it as a sheet
  // from the bottom, with the ways in. Facts load through `prCard.svelte.ts`; what Hub already
  // knows of the pull request fills the card meanwhile, and stays when GitHub cannot be reached.
  import type { PullCard } from "../bindings/PullCard";
  import type { ThreadPr } from "../bindings/ThreadPr";
  import { ago } from "../format";
  import { openUrl } from "../native";
  import { portal } from "../portal";
  import { hub } from "../state.svelte";
  import { lastCard, loadCard, needsNewer, type CardKey } from "../prCard.svelte";

  let {
    anchor,
    cardKey,
    known,
    owner,
    sheet = false,
    onkeep,
    onleave,
    onclose,
    onopen,
  }: {
    anchor: HTMLElement;
    cardKey: CardKey;
    /** The pull request of a Hub thread with this number, when there is one. */
    known?: ThreadPr;
    /** Its thread's title, when a Hub thread owns it. */
    owner?: string;
    sheet?: boolean;
    onkeep: () => void;
    onleave: () => void;
    onclose: () => void;
    /** Opens it in Hub's own pull request view; absent when no project has it. */
    onopen?: () => void;
  } = $props();

  let card = $state<PullCard | null>(null);
  $effect.pre(() => {
    card ??= lastCard(cardKey);
  });
  let failed = $state<"" | "old" | "offline">("");
  let box = $state<HTMLElement>();
  let pos = $state({ left: 0, top: 0 });

  $effect(() => {
    let gone = false;
    loadCard(cardKey)
      .then((c) => !gone && ((card = c), (failed = "")))
      .catch((e) => !gone && (failed = needsNewer(e) ? "old" : "offline"));
    return () => (gone = true);
  });

  // Under the reference, or above it when there is no room, inside the window.
  $effect(() => {
    void card;
    if (sheet || !box) return;
    if (!anchor.isConnected) return onclose();
    const zoom = "__TAURI_INTERNALS__" in window ? 1 : Number(document.documentElement.style.zoom) || 1;
    const r = anchor.getBoundingClientRect();
    const vw = innerWidth / zoom, vh = innerHeight / zoom;
    const w = box.offsetWidth, h = box.offsetHeight;
    const below = r.bottom / zoom + 6;
    const top = below + h <= vh - 8 ? below : Math.max(8, r.top / zoom - h - 6);
    pos = { left: Math.max(8, Math.min(r.left / zoom, vw - w - 8)), top };
  });

  // Scrolling the chat moves the reference away from the card.
  $effect(() => {
    if (sheet) return;
    const scrolled = (e: Event) => !(e.target instanceof Node && box?.contains(e.target)) && onclose();
    document.addEventListener("scroll", scrolled, true);
    return () => document.removeEventListener("scroll", scrolled, true);
  });

  const status = $derived(card?.state ?? known?.state ?? "");
  const title = $derived(card?.title ?? known?.title ?? "");
  const url = $derived(card?.url ?? known?.url ?? (cardKey.repo ? `https://github.com/${cardKey.repo}/pull/${cardKey.number}` : ""));
  const where = $derived(card?.repo || cardKey.repo || "");
  const WORD: Record<string, string> = { open: "Open", draft: "Draft", merged: "Merged", closed: "Closed" };
  const checks = $derived(
    !card || card.checks === "none"
      ? ""
      : card.checks === "pass"
        ? "All checks passed"
        : card.checks === "fail"
          ? "A check failed"
          : `Checks running · ${card.checks_done} of ${card.checks_total}`,
  );

  function github() {
    if (url) openUrl(url).catch((e) => hub.fail(e));
    onclose();
  }
</script>

{#snippet body()}
  <div class="top">
    <span class="pill {status}">
      <svg viewBox="0 0 24 24" class="g"><path d="M6 3a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM6 7v10M6 17a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM18 17a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM18 17V9a3 3 0 0 0-3-3h-4M13 4l-2 2 2 2" /></svg>
      {WORD[status] ?? "Pull request"}
    </span>
    <span class="where">{where ? `${where} ` : ""}#{cardKey.number}</span>
    {#if card}<span class="age">{ago(card.updated_at)}</span>{/if}
  </div>
  {#if title}
    <div class="title">{title}</div>
  {:else if failed}
    <div class="title mute">{failed === "old" ? "That machine's Hub needs the next version to show this" : "Can't reach GitHub right now"}</div>
  {:else}
    <div class="shimmer"></div>
    <div class="shimmer short"></div>
  {/if}
  {#if card}
    <div class="row">
      <span class="who">{card.author}</span>
      <span class="chip"><span class="add">+{card.additions}</span> <span class="del">−{card.deletions}</span></span>
      <span class="chip">{card.files} {card.files === 1 ? "file" : "files"}</span>
    </div>
    {#if checks || owner}
      <div class="foot">
        {#if checks}<span class="chk {card.checks}"><i></i>{checks}</span>{/if}
        {#if owner}<span class="owner" title={owner}>{owner}</span>{/if}
      </div>
    {/if}
  {:else if failed && !title}
    <div class="row">{#if url}<button class="link" onclick={github}>Open on GitHub</button>{/if}</div>
  {/if}
  {#if sheet}
    <div class="acts">
      {#if onopen}<button class="pri" onclick={() => (onopen(), onclose())}>Open PR</button>{/if}
      {#if url}<button onclick={github}>GitHub</button>{/if}
    </div>
  {/if}
{/snippet}

{#if sheet}
  <div class="scrim" use:portal in:fade={{ duration: 160 }} out:fade={{ duration: 130 }} role="presentation" onclick={onclose}></div>
  <div class="card sheet" use:portal in:slide={{ y: 24 }} out:slide={{ y: 24, out: true }} bind:this={box} role="dialog" aria-label="Pull request {cardKey.number}">
    <div class="grab"></div>
    {@render body()}
  </div>
{:else}
  <div
    class="card"
    use:portal
    bind:this={box}
    style:left="{pos.left}px"
    style:top="{pos.top}px"
    role="tooltip"
    onmouseenter={onkeep}
    onmouseleave={onleave}
  >
    {@render body()}
  </div>
{/if}

<style>
  .card {
    position: fixed; z-index: 60; width: calc(350px * var(--text-grow)); max-width: calc(100vw - 16px); padding: 12px 14px;
    background: var(--surface); border: 1px solid var(--line-strong); border-radius: var(--r-lg);
    box-shadow: 0 14px 40px rgba(0, 0, 0, 0.45); font-size: 13px; color: var(--text-2);
    animation: rise var(--dur-fast) var(--ease);
  }
  @keyframes rise { from { opacity: 0; transform: translateY(3px); } }
  @media (prefers-reduced-motion: reduce) { .card { animation: none; } }
  .sheet { left: 0; right: 0; bottom: 0; top: auto; width: auto; max-width: none; border-radius: 18px 18px 0 0; border-bottom: 0; padding: 10px 16px calc(18px + env(safe-area-inset-bottom)); z-index: 61; }
  .scrim { position: fixed; inset: 0; z-index: 60; background: rgba(0, 0, 0, 0.35); }
  .grab { width: 36px; height: 4px; border-radius: 2px; background: var(--line-strong); margin: 0 auto 12px; }
  .top { display: flex; align-items: center; gap: 8px; color: var(--muted); font-size: 12px; }
  .age { margin-left: auto; }
  .where { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .pill { display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; border-radius: 999px; font-weight: 500; background: var(--surface-2); color: var(--muted); flex: none; }
  .pill .g { width: 12px; height: 12px; fill: none; stroke: currentColor; stroke-width: 1.9; stroke-linecap: round; stroke-linejoin: round; }
  .pill.open { color: var(--green); background: var(--green-bg); }
  .pill.merged { color: var(--accent); background: var(--accent-bg); }
  .pill.closed { color: var(--red); background: var(--red-bg); }
  .title { margin: 10px 0; font-size: 14px; line-height: 1.4; color: var(--text); overflow-wrap: anywhere; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .title.mute { color: var(--muted); }
  .row { display: flex; align-items: center; gap: 6px; }
  .who { margin-right: auto; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); }
  .chip { font-size: 12px; padding: 2px 8px; border-radius: var(--r-sm, 7px); background: var(--surface-2); color: var(--muted); white-space: nowrap; }
  .add { color: var(--green); }
  .del { color: var(--red); }
  .foot { display: flex; align-items: center; gap: 8px; margin-top: 10px; padding-top: 8px; border-top: 1px solid var(--line); font-size: 12px; color: var(--muted); }
  .chk { display: inline-flex; align-items: center; gap: 8px; }
  .chk i { width: 7px; height: 7px; border-radius: 50%; background: var(--muted); }
  .chk.pass i { background: var(--green); }
  .chk.fail i { background: var(--red); }
  .chk.pending i { background: var(--amber); }
  .owner { margin-left: auto; max-width: 55%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; border: 1px solid var(--line-strong); border-radius: var(--r-xs); padding: 0 4px; font-size: 11px; }
  .shimmer { height: 10px; border-radius: var(--r-xs); background: var(--surface-2); margin: 8px 0; }
  .shimmer.short { width: 60%; }
  .link { background: none; border: 0; padding: 0; font-size: 12px; color: var(--accent); }
  .acts { display: flex; gap: 6px; margin-top: 12px; }
  .acts button { font-size: 13px; padding: 6px 12px; border-radius: var(--r-md); border: 1px solid var(--line-strong); background: var(--surface-2); color: var(--text-2); }
  .acts .pri { background: var(--accent-bg); border-color: transparent; color: var(--accent); }
</style>
