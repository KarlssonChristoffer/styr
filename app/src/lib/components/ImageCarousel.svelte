<!-- The pictures the agent made in a turn, in the reply, bare: no frame and no buttons, the way Claude Code shows them. One picture is shown big; two or three sit in a row at one height; more than three take the room of one, with arrows and a strip of the others under it. A click opens the picture over the whole window (Lightbox.svelte), where the tools are.
     Used by ChildLine.svelte, Transcript.svelte. -->
<script lang="ts">
  // The pictures the agent made in a turn, in the reply, bare: no frame and no buttons, the way
  // Claude Code shows them. One picture is shown big; two or three sit in a row at one height;
  // more than three take the room of one, with arrows and a strip of the others under it. A click
  // opens the picture over the whole window (Lightbox.svelte), where the tools are.
  import type { ImageItem } from "../transcript";
  import { openPicture } from "../lightbox.svelte";
  import { menu } from "../menu.svelte";
  import { imageMenu } from "../menus.svelte";
  import Icon from "./Icon.svelte";

  let { images, threadId }: { images: ImageItem[]; threadId: string } = $props();
  let at = $state(0);
  // A turn that is still working may add pictures; the one you were on stays.
  const image = $derived(images[Math.min(at, images.length - 1)]);
  const many = $derived(images.length > 3);
  const nameOf = (i: ImageItem) => i.path?.split(/[\\/]/).pop() ?? "Image";

  // A picture's shape before it has loaded, so the thread is already as tall as it will be when it
  // scrolls to the end. A PNG says so in its header; anything else is taken as 4:3.
  function ratio(i: ImageItem): number {
    const head = i.src.startsWith("data:image/png;base64,") ? atob(i.src.slice(22, 54)) : "";
    const word = (k: number) => ((head.charCodeAt(k) << 24) | (head.charCodeAt(k + 1) << 16) | (head.charCodeAt(k + 2) << 8) | head.charCodeAt(k + 3)) >>> 0;
    return head.length >= 24 && word(16) && word(20) ? word(16) / word(20) : 4 / 3;
  }
  // Two or three in a row share one height: each takes width in step with its shape. A row of tall
  // pictures stops before it gets taller than about 300 px.
  const sum = $derived(images.reduce((n, i) => n + ratio(i), 0));

  const open = (i: ImageItem) => openPicture(threadId, i.id);
  const step = (by: number) => (at = (at + by + images.length) % images.length);
</script>

{#if images.length === 1 || many}
  <figure class="one" use:menu={() => imageMenu(image.src, image.path, () => open(image))}>
    <div class="stage">
      <button class="pic" onclick={() => open(image)} data-tip={image.path ?? nameOf(image)}>
        <img src={image.src} alt={image.prompt ?? nameOf(image)} style:aspect-ratio={ratio(image)} />
      </button>
      {#if many}
        <button class="arrow prev" data-tip="Previous" aria-label="Previous" onclick={() => step(-1)}><Icon name="back" size={14} /></button>
        <button class="arrow next" data-tip="Next" aria-label="Next" onclick={() => step(1)}><Icon name="chevron" size={14} /></button>
        <span class="count">{Math.min(at, images.length - 1) + 1} / {images.length}</span>
      {/if}
    </div>
    {#if many}
      <div class="strip">
        {#each images as other, k (other.id)}
          <button class:on={other === image} aria-label="Picture {k + 1}" onclick={() => (at = k)}><img src={other.src} alt="" /></button>
        {/each}
      </div>
    {/if}
    {#if image.prompt}<figcaption data-tip={image.prompt}>{image.prompt}</figcaption>{/if}
  </figure>
{:else}
  <div class="row" style:max-width="min(100%, {Math.round(sum * 300)}px)">
    {#each images as i (i.id)}
      <button class="pic cell" style:flex-grow={ratio(i)} onclick={() => open(i)} data-tip={i.path ?? nameOf(i)} use:menu={() => imageMenu(i.src, i.path, () => open(i))}>
        <img src={i.src} alt={i.prompt ?? nameOf(i)} style:aspect-ratio={ratio(i)} />
      </button>
    {/each}
  </div>
{/if}

<style>
  .one { margin: 6px 0 12px; display: flex; flex-direction: column; gap: 6px; max-width: min(680px, 100%); }
  .stage { position: relative; align-self: flex-start; max-width: 100%; }
  .pic { display: block; padding: 0; border-radius: var(--r-lg); overflow: hidden; background: var(--surface); cursor: zoom-in; }
  .pic img { display: block; max-width: 100%; max-height: 460px; width: auto; height: auto; object-fit: contain; }
  .pic:hover img { filter: brightness(1.06); }
  .arrow {
    position: absolute; top: 50%; transform: translateY(-50%); display: grid; place-items: center;
    width: 30px; height: 30px; border-radius: 50%; background: rgba(10, 10, 9, 0.62); color: #fff;
    opacity: 0; transition: opacity 0.12s var(--ease);
  }
  .stage:hover .arrow, .arrow:focus-visible { opacity: 1; }
  .arrow:hover { background: rgba(10, 10, 9, 0.82); }
  .prev { left: 10px; }
  .next { right: 10px; }
  .count { position: absolute; right: 10px; bottom: 10px; padding: 1px 8px; border-radius: var(--r-lg); background: rgba(10, 10, 9, 0.62); color: #fff; font-size: 12px; font-variant-numeric: tabular-nums; pointer-events: none; }
  .strip { display: flex; gap: 6px; overflow-x: auto; padding: 2px; }
  .strip button { flex: none; width: 64px; height: 40px; padding: 0; border-radius: var(--r-sm); overflow: hidden; opacity: 0.45; }
  .strip button:hover { opacity: 0.8; }
  .strip button.on { opacity: 1; box-shadow: 0 0 0 1.5px var(--text-3); }
  .strip img { display: block; width: 100%; height: 100%; object-fit: cover; object-position: top left; }
  figcaption {
    color: var(--muted); font-size: 12px; line-height: 1.45;
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }
  .row { display: flex; gap: 8px; margin: 6px 0 12px; }
  .cell { flex-basis: 0; min-width: 0; }
  .cell img { width: 100%; max-height: none; }
</style>
