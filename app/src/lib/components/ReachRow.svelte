<!-- För: agent. The folders a thread can also look in (`thread.reach`), as chips under the composer. -->
<script lang="ts">
  import { untrack } from "svelte";
  import Icon from "./Icon.svelte";
  import { api } from "../api";
  import { hubdHas } from "../compat";
  import { folderName } from "../format";
  import { quiet } from "../quiet";
  import type { ThreadReach } from "../bindings/ThreadReach";

  /** What the thread reaches beyond its folder (`reach.rs`). In a thread it lives in the core and is
   *  read when the box opens; in a new thread's box it waits in `reach` for the thread to start. */
  let { reach = $bindable({}), threadId, onerror }: { reach?: ThreadReach; threadId?: string; onerror?: (e: unknown) => void } = $props();

  $effect(() => {
    const id = threadId;
    if (!id) return;
    untrack(() => (reach = {}));
    hubdHas("thread.reach").then((ok) => { // compat-until: 2026-12-03
      if (ok && id === threadId) quiet(api.threadReach(id).then((r) => id === threadId && (reach = r)), "thread reach");
    });
  });

  /** Sets what the thread reaches (also called by the + menu). */
  export async function change(next: ThreadReach) {
    reach = threadId ? await api.setThreadReach(threadId, next) : next;
  }
</script>

{#if reach.folders?.length}
  <div class="chips reach">
    {#each reach.folders as f (f.path)}
      <span class="chip" data-tip="The agent can also look in {f.path}">
        <span class="look"><Icon name="branch" size={13} /><span class="name">{f.name ?? folderName(f.path)}</span></span>
        <button class="remove" data-tip="Remove" aria-label="Remove {f.name ?? folderName(f.path)}" onclick={() => change({ ...reach, folders: reach.folders!.filter((g) => g.path !== f.path) }).catch((e) => onerror?.(e))}><Icon name="x" size={12} /></button>
      </span>
    {/each}
  </div>
{/if}

<style>
  /* @kit chips */
</style>
