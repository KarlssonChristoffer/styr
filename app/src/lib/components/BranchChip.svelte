<!-- The thread's branch in its header. Click to switch to another branch, or make a new one from here; uncommitted changes come along, and git refuses when they would be lost.
     Used by ThreadView.svelte. -->
<script lang="ts">
  import { autofocus } from "../autofocus";
  // The thread's branch in its header. Click to switch to another branch, or make a new one from
  // here; uncommitted changes come along, and git refuses when they would be lost.
  import { api } from "../api";
  import type { Thread } from "../bindings/Thread";
  import { contextMenu } from "../menu.svelte";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let { thread, branch, onswitched }: { thread: Thread; branch: string; onswitched?: () => void } = $props();

  let naming = $state(false);
  let name = $state("");

  async function to(b: string, create = false) {
    try {
      await api.switchBranch(thread.id, b, create);
      naming = false;
      name = "";
      onswitched?.();
    } catch (e) {
      hub.fail(e);
    }
  }

  async function pick(e: MouseEvent) {
    if (thread.status === "running") return hub.fail(new Error("Switch branch when the turn has ended."));
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const zoom = Number(document.documentElement.style.zoom) || 1;
    const all = await api.localBranches(thread.id).catch(() => [] as string[]);
    contextMenu.show(r.left / zoom, r.bottom / zoom + 4, [
      { label: "New branch from here…", run: () => (naming = true) },
      null,
      ...all.slice(0, 15).map((b) => ({ label: b, hint: b === branch ? "✓" : undefined, disabled: b === branch, run: () => to(b) })),
    ]);
  }
</script>

{#if naming}
  <input
    class="name mono"
    bind:value={name}
    placeholder="new-branch-name"
    spellcheck="false"
    use:autofocus
    onkeydown={(e) => (e.key === "Enter" && name.trim() ? to(name.trim(), true) : e.key === "Escape" && (naming = false))}
    onblur={() => (naming = false)}
  />
{:else}
  <button class="branch mono" data-tip="Git branch · click to switch" onclick={pick}><Icon name="branch" size={12} />{branch}</button>
{/if}

<style>
  .branch { display: inline-flex; align-items: center; gap: 4px; color: var(--text-3); white-space: nowrap; flex: none; font-size: inherit; padding: 0; }
  .branch:hover { color: var(--text); }
  .name { width: 180px; background: var(--surface-2); border: 1px solid var(--accent); border-radius: var(--r-sm); padding: 0 6px; font-size: 12px; color: var(--text-2); outline: none; }
</style>
