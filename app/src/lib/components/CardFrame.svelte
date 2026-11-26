<!-- The frame the board puts around every Overview card: its title, when you have switched it on in the menu (in your own words, or the card's name), the card's content, and the text behind How it's counted when the menu asks for it. The card itself is only its content: no heading, no buttons, no pickers; those are the frame's and live in the right-click menu (`CardMenu.svelte`, `overview.svelte.ts`). Looks set in the menu reach this section through the slot's classes, so a card needs no code for them either.
     Used by Board.svelte. -->
<script lang="ts">
  // The frame the board puts around every Overview card: its title, when you have switched it on
  // in the menu (in your own words, or the card's name), the card's content, and the text behind
  // How it's counted when the menu asks for it. The card itself is only its content: no heading,
  // no buttons, no pickers; those are the frame's and live in the right-click menu
  // (`CardMenu.svelte`, `overview.svelte.ts`). Looks set in the menu reach this section through
  // the slot's classes, so a card needs no code for them either.
  import type { Snippet } from "svelte";
  import { about, frameTitle } from "../overview.svelte";
  import { prefs } from "../prefs.svelte";

  let { id, name, open = false, shut, children }: {
    id: string;
    /** The card's name, which the title says unless you wrote one. */
    name: string;
    /** Whether the card's How it's counted text is open. */
    open?: boolean;
    shut?: () => void;
    children: Snippet;
  } = $props();

  const title = $derived.by(() => {
    void prefs.overviewFrames;
    return frameTitle(id, name);
  });
</script>

<section class="frame">
  {#if title}<h3 class="frame-title">{title}</h3>{/if}
  {@render children()}
  {#if open && about[id]}
    <div class="about">
      {@render about[id]()}
      <button class="shut" onclick={shut} aria-label="Close">Close</button>
    </div>
  {/if}
</section>

<style>
  section { background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); padding: 14px; height: 100%; min-width: 0; container-type: inline-size; }
  .frame-title { color: var(--text); font-size: 13px; font-weight: 500; margin: 0 0 10px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  /* The one line of text under a picture, and the one figure a card says big: the same in every
     card, so the cards read as one page. */
  section :global(.caption) { margin: 10px 0 0; font-size: 12px; line-height: 1.35; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  section :global(.caption em) { font-style: normal; color: var(--text-2); font-weight: 500; }
  section :global(.caption .plus) { color: var(--green); }
  section :global(.caption .minus) { color: var(--red); }
  section :global(.figure) { font-size: 22px; font-weight: 500; color: var(--text); letter-spacing: -0.01em; font-variant-numeric: tabular-nums; line-height: 1.1; }
  section :global(.quiet) { color: var(--muted); font-size: 12px; margin: 0; }
  .about { position: relative; color: var(--text-3); font-size: 12px; line-height: 1.5; background: var(--bg); border-radius: var(--r-md); padding: 8px 10px 26px; margin-top: 10px; }
  .about :global(p) { margin: 0 0 4px; }
  .about :global(p:last-child) { margin: 0; }
  .about :global(b) { color: var(--text-2); font-weight: 500; }
  .shut { position: absolute; right: 8px; bottom: 5px; font-size: 11px; color: var(--muted); }
  .shut:hover { color: var(--text); }
</style>
