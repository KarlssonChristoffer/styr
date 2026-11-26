<!-- The list of put-away prompts over the message box (opened by Ctrl S or the Stash tab).
     Used by Composer.svelte. -->
<script lang="ts">
  // The list of put-away prompts over the message box (opened by Ctrl S or the Stash tab).
  import { pop } from "../popover";
  import { ago } from "../format";
  import { stash } from "../stash.svelte";
  import type { StashList } from "../composerHistory.svelte";
  import Icon from "./Icon.svelte";

  let { list }: { list: StashList } = $props();
</script>

{#if stash.list.length && list.open}
  <div class="stash">
    {#if list.open}
      <div class="stash-list" transition:pop|global role="listbox" aria-label="Put away">
        <button class="stash-head" onclick={() => (list.open = false)} data-tip="Close (Esc)">
          <Icon name="bookmark" size={13} /><span>Stash</span><span class="grow"></span><span class="n">{stash.list.length}</span><Icon name="down" size={13} />
        </button>
        {#each stash.list as s, i (s.at + ":" + i)}
          <div class="stash-row" class:active={i === list.at} role="option" aria-selected={i === list.at} tabindex="-1" onpointerenter={() => (list.at = i)} onpointerdown={(e) => e.preventDefault()} onclick={() => list.restore(i)} onkeydown={() => {}}>
            <Icon name="file" size={13} />
            <span class="first">{(s.text.trim().split("\n")[0] || s.attachments.map((a) => a.name).join(", ")).slice(0, 90)}</span>
            <span class="when">{ago(s.at) === "now" ? "just now" : `${ago(s.at)} ago`}</span>
            <button class="gone" aria-label="Remove" data-tip="Remove (Delete)" onpointerdown={(e) => e.stopPropagation()} onclick={(e) => { e.stopPropagation(); list.remove(i); }}><Icon name="x" size={12} /></button>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .stash { position: absolute; right: 0; left: 0; bottom: 100%; z-index: 4; pointer-events: none; }
  .stash > * { pointer-events: auto; }
  .stash-list {
    position: absolute; left: 28px; right: 28px; bottom: 0; padding: 4px 6px 6px; background: var(--surface);
    border: 1px solid var(--line-strong); border-bottom: none; border-radius: var(--r-lg) var(--r-lg) 0 0; max-height: 40vh; overflow-y: auto;
  }
  .stash-head { display: flex; align-items: center; gap: 8px; width: 100%; padding: 6px 10px; color: var(--muted); font-size: 13px; text-align: left; }
  .stash-head:hover { color: var(--text); }
  .stash-row { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: var(--r-md); font-size: 14px; color: var(--text-2); cursor: pointer; }
  .stash-row.active { background: var(--surface-hover); color: var(--text); }
  .stash-row .first { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .stash-row .when { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .stash-row .gone { color: var(--muted); display: inline-flex; padding: 2px; border-radius: var(--r-sm); }
  .stash-row .gone:hover { color: var(--text); background: var(--line); }
  .grow { flex: 1; }
</style>
