<!-- The chips for what is attached above the composer box, and the look at one: remove it, put its text in the box, or peek at it. The list stays with Composer.
     Used by Composer.svelte. -->
<script lang="ts">
  import { pop } from "../popover";
  import { lineCount, preview, size, THREAD_MIME } from "../attachments";
  import { here, putInBag } from "../bag.svelte";
  import { peekText } from "../composerFiles";
  import type { Attachment } from "../bindings/Attachment";
  import Icon from "./Icon.svelte";

  /** The chips above the message box, and the look at one before it goes. */
  let {
    attachments,
    peek = $bindable(null),
    threadId,
    cwd,
    onremove,
    oninline,
    onerror,
  }: {
    attachments: Attachment[];
    /** The chip opened for a look, by position. */
    peek: number | null;
    threadId?: string;
    cwd?: string;
    onremove: (i: number) => void;
    /** Puts the text of the attachment at `i` in the box. */
    oninline: (i: number) => void;
    onerror?: (e: unknown) => void;
  } = $props();

  const peekAt = $derived(peek !== null ? attachments[peek] : undefined);
</script>

{#if attachments.length}
  <div class="chips">
    {#each attachments as a, i (i)}
      {@const lines = lineCount(a)}
      <span class="chip" class:open={peek === i}>
        <button class="look" data-tip={a.path ? a.path : a.url ? a.url : `Look at ${a.name}`} disabled={!preview(a) && !a.text} onclick={() => (peek = peek === i ? null : i)}>
          {#if preview(a)}<img src={preview(a)} alt="" />{:else}<Icon name={a.url ? "link" : "file"} size={13} />{/if}
          <span class="name">{a.name}</span><span class="size">{lines > 1 ? `${lines.toLocaleString()} lines · ` : ""}{size(a)}</span>
        </button>
        {#if a.mime !== THREAD_MIME}
          <button class="remove keep" data-tip="Also keep it in the context bag, for other threads" onclick={() => putInBag($state.snapshot(a) as Attachment, here(threadId, cwd)).catch((e) => onerror?.(e))}><Icon name="bag" size={12} /></button>
        {/if}
        <button class="remove" data-tip="Remove" onclick={() => onremove(i)}><Icon name="x" size={12} /></button>
      </span>
    {/each}
    {#if peekAt}
      {@const a = peekAt}
      <div class="peek" transition:pop|global role="dialog" aria-label={a.name}>
        <div class="peek-head">
          <span class="name mono">{a.name}</span>
          <span class="grow"></span>
          {#if a.text && a.mime !== THREAD_MIME}
            <button class="link" data-tip="Puts the text in the message box where the cursor is" onclick={() => oninline(peek!)}>Put in the box</button>
          {/if}
          <button class="link quiet" onclick={() => onremove(peek!)}>Remove</button>
          <button class="remove" data-tip="Close (Esc)" onclick={() => (peek = null)}><Icon name="x" size={12} /></button>
        </div>
        {#if preview(a)}
          <img src={preview(a)} alt={a.name} />
        {:else}
          <pre>{peekText(a.text ?? "")}</pre>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  /* @kit chips */
  .chip.open { box-shadow: inset 0 0 0 1px var(--line-strong); }
  .look:disabled { cursor: default; }
  .look:not(:disabled):hover .name { color: var(--text); }
  .chip img { width: 22px; height: 22px; object-fit: cover; border-radius: var(--r-xs); }
  .chip .size { color: var(--muted); }
  .peek {
    position: absolute; z-index: 20; left: 0; right: 0; bottom: calc(100% + 6px);
    background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45); display: flex; flex-direction: column; max-height: 360px;
  }
  .peek-head { display: flex; align-items: center; gap: 10px; padding: 6px 6px 6px 10px; border-bottom: 1px solid var(--line-strong); font-size: 12px; }
  .peek-head .name { color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .peek-head .link { color: var(--accent); padding: 1px 6px; border-radius: var(--r-sm); }
  .peek-head .link.quiet { color: var(--muted); }
  .peek-head .link:hover { background: var(--surface-hover); }
  .peek pre { margin: 0; padding: 8px 10px; overflow: auto; font-size: 12px; line-height: 1.45; color: var(--text-2); white-space: pre; }
  .peek img { max-width: 100%; max-height: 320px; object-fit: contain; align-self: center; padding: 8px; }
  .grow { flex: 1; }
</style>
