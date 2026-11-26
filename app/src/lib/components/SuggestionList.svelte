<!-- The autocomplete list above the composer: shows the suggestions a `Suggest` holds and reports a pick. It owns no suggestion logic.
     Used by Composer.svelte. -->
<script lang="ts">
  import { pop } from "../popover";
  import Icon from "./Icon.svelte";
  import type { Suggest } from "../suggest.svelte";

  let { suggest }: { suggest: Suggest } = $props();
</script>

{#if suggest.list.length}
  <div class="suggest" transition:pop|global role="listbox">
    {#each suggest.list as s, i (`${i}:${s.label}`)}
      <button
        class="option"
        class:active={i === suggest.active}
        role="option"
        aria-selected={i === suggest.active}
        onpointerdown={(e) => {
          e.preventDefault();
          s.apply();
        }}
        onpointerenter={() => (suggest.active = i)}
      >
        {#if s.thread}<Icon name="threads" size={12} />{/if}<span class:mono={!s.thread}>{s.label}</span>{#if s.hub}<span class="tag">Hub</span>{/if}{#if s.detail}<span class="detail">{s.detail}{#if s.remove && i === suggest.active}{" · Delete removes it"}{/if}</span>{/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .suggest {
    position: absolute; z-index: 20; bottom: calc(100% + 4px); left: 8px; right: 8px;
    background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg);
    padding: 4px; box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    max-height: 340px; overflow-y: auto;
  }
  .option {
    display: flex; align-items: baseline; gap: 10px; width: 100%; text-align: left;
    padding: 6px 8px; border-radius: var(--r-md); color: var(--text-2);
  }
  .option:hover, .option.active { background: var(--surface-hover); color: var(--text); }
  .mono { color: var(--text); white-space: nowrap; }
  .tag { font-size: 11px; color: var(--muted); border: 1px solid var(--line-strong); border-radius: 999px; padding: 0 6px; line-height: 16px; }
  .detail { color: var(--muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
