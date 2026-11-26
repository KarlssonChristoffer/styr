<!-- The + button of the composer and the menu it opens (PlusMenu), with what each choice does to the box.
     Used by Composer.svelte. -->
<script lang="ts">
  import { pop } from "../popover";
  import { here } from "../bag.svelte";
  import type { Attachment } from "../bindings/Attachment";
  import type { ThreadReach } from "../bindings/ThreadReach";
  import Icon from "./Icon.svelte";
  import PlusMenu from "./PlusMenu.svelte";

  let {
    menu = $bindable(null),
    threadId = null,
    engine,
    images = true,
    web,
    cwd = "",
    host = null,
    reach = {},
    onreach,
    onfiles,
    onbag,
    onthread,
    ondiff,
    onweb,
    onfocus,
  }: {
    /** Which menu of the bar is open; this one is "plus". */
    menu?: string | null;
    threadId?: string | null;
    engine: string;
    images?: boolean;
    web: boolean;
    cwd?: string;
    host?: string | null;
    reach?: ThreadReach;
    onreach?: (next: ThreadReach) => Promise<void> | void;
    onfiles: () => void;
    onbag: (items: Attachment[]) => void;
    onthread: (t: { id: string; title: string }) => void;
    ondiff: () => void;
    onweb: (on: boolean) => void;
    /** Gives the focus back to the text box. */
    onfocus: () => void;
  } = $props();
</script>

<!-- What goes along: files, the bag, another thread, what the thread can reach (PlusMenu). -->
<div class="menu-wrap plus-wrap">
  <button class="tool plus" class:open={menu === "plus"} data-tip="Add context: files, the bag, another thread, connections" aria-label="Add context" aria-haspopup="menu" aria-expanded={menu === "plus"} onclick={() => (menu = menu === "plus" ? null : "plus")}>
    <Icon name="plus" size={16} />
  </button>
  {#if menu === "plus"}
    <div class="plus-pos" transition:pop|global>
      <PlusMenu
        {threadId}
        {engine}
        from={here(threadId, cwd)}
        {images}
        {web}
        canDiff={!!cwd}
        {cwd}
        {host}
        {reach}
        {onreach}
        onfiles={() => {
          menu = null;
          onfiles();
        }}
        onbag={(items) => {
          onbag(items);
          menu = null;
          queueMicrotask(onfocus);
        }}
        onthread={(t) => {
          menu = null;
          onthread(t);
        }}
        ondiff={() => {
          menu = null;
          ondiff();
        }}
        {onweb}
        onclose={() => {
          menu = null;
          queueMicrotask(onfocus);
        }}
      />
    </div>
  {/if}
</div>

<style>
  /* The + menu draws its own card and lists beside it, so nothing here clips them. */
  .plus-pos { position: absolute; z-index: 20; bottom: calc(100% + 6px); left: 0; }
  .tool.plus { width: 30px; justify-content: center; padding: 0; margin-right: 3px; color: var(--text-3); }
  .tool.plus.open { background: var(--surface-hover); color: var(--text); }
</style>
