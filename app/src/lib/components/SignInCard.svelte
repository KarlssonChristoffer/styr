<!-- An engine on another machine is waiting to be signed in with a link and a code (`autologin.rs`): one row on Home with the code, "Open link" and "Later". Later hides this code only; a new code shows again. The row leaves by itself once the machine is signed in.
     Used by Home.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  // An engine on another machine is waiting to be signed in with a link and a code (`autologin.rs`):
  // one row on Home with the code, "Open link" and "Later". Later hides this code only; a new code
  // shows again. The row leaves by itself once the machine is signed in.
  import { onMount } from "svelte";
  import { api } from "../api";
  import { hub } from "../state.svelte";
  import { copy } from "../menu.svelte";
  import { openUrl } from "../native";
  import type { PendingSignIn } from "../bindings/PendingSignIn";

  const LATER = "hub.signin.later";
  let list = $state<PendingSignIn[]>([]);
  let later = $state<string[]>(JSON.parse(localStorage.getItem(LATER) ?? "[]"));
  let busy = $state<string | null>(null);

  const key = (p: PendingSignIn) => `${p.machine}/${p.engine}/${p.code ?? ""}`;
  const shown = $derived(list.filter((p) => !later.includes(key(p))).slice(0, 3));

  async function look() {
    list = await api.pendingSignIns();
  }
  onMount(() => {
    look();
    const timer = poll(look, 10_000);
    return () => timer();
  });

  function hide(p: PendingSignIn) {
    later = [...later.slice(-30), key(p)];
    localStorage.setItem(LATER, JSON.stringify(later));
  }

  async function again(p: PendingSignIn) {
    busy = key(p);
    try {
      await api.startLogin(p.engine, p.machine);
      await look();
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = null;
    }
  }
</script>

{#each shown as p (p.machine + p.engine)}
  <div class="signin">
    <span class="dot"></span>
    {#if p.running && p.url && p.code}
      <span class="what"><b>{p.label} on {p.machine}</b> is not signed in. Open the link and type the code.</span>
      <button class="code mono" data-tip="Copy" onclick={() => copy(p.code!)}>{p.code}</button>
      <button class="go" onclick={() => openUrl(p.url!).catch((e) => hub.fail(e))}>Open link</button>
    {:else if p.running}
      <span class="what"><b>{p.label} on {p.machine}</b> is not signed in. {p.message}</span>
    {:else}
      <span class="what"><b>{p.label} on {p.machine}</b> is not signed in. The code ran out.</span>
      <button class="go" disabled={busy === key(p)} onclick={() => again(p)}>{busy === key(p) ? "Asking…" : "New code"}</button>
    {/if}
    <button class="later" onclick={() => hide(p)}>Later</button>
  </div>
{/each}

<style>
  .signin { display: flex; align-items: center; gap: 10px; background: var(--surface); border: 1px solid var(--line-strong); border-radius: var(--r-lg); padding: 8px 10px 8px 12px; margin-bottom: 12px; font-size: 13px; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--amber); flex: none; }
  .what { flex: 1; min-width: 0; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .what b { color: var(--text); font-weight: 500; }
  .code { color: var(--text); background: var(--surface-hover); padding: 2px 8px; border-radius: var(--r-sm); font-size: 13px; letter-spacing: 0.08em; }
  .go { color: var(--accent); background: var(--accent-bg); padding: 3px 10px; border-radius: var(--r-sm); font-size: 13px; }
  .go:disabled { opacity: 0.6; }
  .later { color: var(--muted); padding: 3px 8px; border-radius: var(--r-sm); font-size: 13px; }
  .later:hover { color: var(--text-2); background: var(--surface-hover); }
</style>
