<!-- A live page an agent showed: a small still preview on its card in the chat (`preview`), or big in the right panel (`full`). The core serves it in a sandbox with an opaque origin (`pages.rs`), so it reaches nothing of Hub's. It only posts three things here, and only messages from this frame count: its height, that it is ready for Hub's colours, and a link to open in the browser.
     Used by FilePreview.svelte, PageCard.svelte, Viewer.svelte, lib/viewkit/Big.svelte. -->
<script lang="ts">
  // A live page an agent showed: a small still preview on its card in the chat (`preview`), or big
  // in the right panel (`full`). The core serves it in a sandbox with an opaque origin
  // (`pages.rs`), so it reaches nothing of Hub's. It only
  // posts three things here, and only messages from this frame count: its height, that it is
  // ready for Hub's colours, and a link to open in the browser.
  import { pageUrl } from "../api";
  import { openUrl } from "../native";

  let {
    threadId,
    id,
    title,
    height,
    full = false,
    preview = false,
    flush = false,
    grow = false,
    flow = false,
  }: {
    threadId: string;
    id: string;
    title: string;
    height?: number;
    /** Fills the right panel. */
    full?: boolean;
    /** Drawn small and still on the page's card in the chat: it cannot be clicked into, and the
     *  card opens it big. */
    preview?: boolean;
    /** Sits inside a Viewkit card: no margin and no corners of its own. */
    flush?: boolean;
    /** Big: as tall as the page says, so the panel scrolls instead of the frame. */
    grow?: boolean;
    /** With `grow`: only as tall as the page says, with no minimum, so a short page leaves no gap. */
    flow?: boolean;
  } = $props();

  const src = $derived(pageUrl(threadId, id));
  // Tall pages scroll inside the frame rather than push the chat away.
  const MAX = 760;
  let frame = $state<HTMLIFrameElement>();
  let reported = $state<number | null>(null);
  let generation = $state(0);
  const shownHeight = $derived(Math.min(Math.max(reported ?? height ?? 320, 60), MAX));

  const TOKENS = ["--bg", "--surface", "--surface-2", "--surface-hover", "--line", "--line-strong", "--text", "--text-2", "--text-3", "--muted", "--accent", "--accent-bg", "--green", "--green-bg", "--amber", "--amber-bg", "--red", "--red-bg", "--radius", "--font", "--mono"];
  function theme() {
    const css = getComputedStyle(document.documentElement);
    return Object.fromEntries(TOKENS.map((t) => [t, css.getPropertyValue(t).trim()]));
  }

  function onMessage(e: MessageEvent) {
    if (!frame || e.source !== frame.contentWindow) return;
    const m = e.data;
    if (!m || typeof m !== "object") return;
    if (m.hub === "height" && typeof m.height === "number" && Number.isFinite(m.height)) reported = Math.ceil(m.height);
    else if (m.hub === "ready") frame.contentWindow?.postMessage({ hub: "theme", vars: theme() }, "*");
    else if (m.hub === "open" && typeof m.url === "string" && /^https?:\/\//i.test(m.url)) void openUrl(m.url);
  }

  /** Loads the page again; the viewer's toolbar has the button when it is shown big. */
  export function reload() {
    reported = null;
    generation++;
  }
</script>

<svelte:window onmessage={onMessage} />

{#if preview}
  <!-- The page laid out twice as wide as the card and drawn at half that: a picture of it. -->
  <div class="preview" inert>
    {#key generation}
      <iframe bind:this={frame} {src} {title} sandbox="allow-scripts" referrerpolicy="no-referrer" loading="lazy" tabindex="-1"></iframe>
    {/key}
  </div>
{:else}
<figure class="page" class:full class:flush class:grow class:flow>
  {#key generation}
    <iframe
      bind:this={frame}
      {src}
      {title}
      sandbox="allow-scripts"
      referrerpolicy="no-referrer"
      loading="lazy"
      style:height={grow ? (reported ? `${reported}px` : undefined) : full ? undefined : `${shownHeight}px`}
    ></iframe>
  {/key}
</figure>
{/if}

<style>
  .page { margin: 6px 0 10px; border-radius: var(--radius); overflow: hidden; background: var(--bg); }
  .page.flush { margin: 0; border-radius: 0; background: transparent; }
  .page.full { margin: 0; border: 0; border-radius: 0; flex: 1; display: flex; flex-direction: column; min-height: 0; }
  iframe { display: block; width: 100%; border: 0; background: transparent; color-scheme: dark; }
  .full iframe { flex: 1; min-height: 0; }
  .full.grow { flex: none; }
  .grow iframe { flex: none; min-height: calc(100vh - 140px); }
  .grow.flow iframe { min-height: 0; }
  /* The preview lays the page out twice as wide and tall as its box and scales it down. */
  .preview { position: absolute; inset: 0; overflow: hidden; pointer-events: none; }
  .preview iframe { width: 200%; height: 200%; transform: scale(0.5); transform-origin: 0 0; }
</style>
