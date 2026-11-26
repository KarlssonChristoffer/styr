<!-- One thing running on a machine that is not an agent session: a service on a port, a build, a container, or the rest of what the machine does. Icon, name, what it takes, how long, and a stop button where Hub may stop it. Read-only rows show a lock instead.
     Used by MachineView.svelte. -->
<script lang="ts">
  // One thing running on a machine that is not an agent session: a service on a port, a build, a
  // container, or the rest of what the machine does. Icon, name, what it takes, how long, and a
  // stop button where Hub may stop it. Read-only rows show a lock instead.
  import { bytes } from "../format";
  import Icon from "./Icon.svelte";
  import type { Snippet } from "svelte";

  let {
    icon,
    tag = null,
    title,
    sub = "",
    mem = 0,
    total = 0,
    cpu = null,
    time = "",
    tone = "var(--text-3)",
    ok = false,
    stop = null,
    stopTip = "Stop it and what it started",
    ask = false,
    lock = false,
    click = null,
  }: {
    icon: Snippet;
    /** A small tag in front of the name, like ":3000". */
    tag?: string | null;
    title: string;
    sub?: string;
    mem?: number;
    total?: number;
    cpu?: number | null;
    time?: string;
    tone?: string;
    /** A green dot: safe to stop. */
    ok?: boolean;
    stop?: (() => Promise<void>) | null;
    stopTip?: string;
    /** Stopping asks for a second click first. */
    ask?: boolean;
    lock?: boolean;
    click?: (() => void) | null;
  } = $props();

  let busy = $state(false);
  let armed = $state(false);
  async function go() {
    if (!stop) return;
    if (ask && !armed) {
      armed = true;
      setTimeout(() => (armed = false), 4000);
      return;
    }
    busy = true;
    armed = false;
    try {
      await stop();
    } finally {
      busy = false;
    }
  }
  const share = $derived(total && mem ? Math.max(2, Math.min(100, (mem / total) * 100)) : 0);
  const pct = (n: number) => `${n >= 10 ? Math.round(n) : n.toFixed(1)}%`;
</script>

<div class="rw" class:lock>
  <span class="k" style:color={tone}>{@render icon()}</span>
  <span class="t">
    {#if tag}<span class="pt">{tag}</span>{/if}
    {#if click}<button class="name" onclick={click}>{title}</button>{:else}{title}{/if}
    {#if sub}<small>{sub}</small>{/if}
  </span>
  <span class="dot" class:g={ok} data-tip={ok ? "Safe to stop" : ""}></span>
  <span class="bar"><i style:width="{share}%" style:background={tone}></i></span>
  <span class="m mono">{mem ? bytes(mem) : ""}</span>
  <span class="c mono" data-tip="Share of the whole machine">{cpu != null ? pct(cpu) : ""}</span>
  <span class="tm mono">{time}</span>
  {#if stop}
    <button class="stop" class:armed disabled={busy} data-tip={armed ? "Click again to stop it" : stopTip} onclick={go}><Icon name="stop" size={13} /></button>
  {:else if lock}
    <span class="lk" data-tip="Not Hub's, so it is only shown"><svg width="12" height="12" viewBox="0 0 24 24"><rect x="5" y="11" width="14" height="9" rx="2" fill="none" stroke="currentColor" stroke-width="2" /><path d="M8 11V8a4 4 0 0 1 8 0v3" fill="none" stroke="currentColor" stroke-width="2" /></svg></span>
  {:else}
    <span class="lk"></span>
  {/if}
</div>

<style>
  .rw { display: grid; grid-template-columns: 18px minmax(0, 1fr) 8px 84px 56px 42px 54px 26px; gap: 10px; align-items: center; padding: 6px 8px; border-radius: var(--r-md); font-size: 13px; }
  .rw:hover { background: var(--surface); }
  .k { display: grid; place-items: center; }
  .t { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--text); min-width: 0; }
  .t small { display: block; color: var(--muted); font-size: 11px; margin-top: 1px; overflow: hidden; text-overflow: ellipsis; }
  .name { color: inherit; padding: 0; font-size: inherit; }
  .name:hover { text-decoration: underline; text-underline-offset: 3px; }
  .pt { display: inline-block; font-family: var(--mono); font-size: 11px; color: var(--text-3); background: var(--surface-hover); border-radius: var(--r-sm); padding: 0 4px; margin-right: 8px; }
  .dot { width: 6px; height: 6px; border-radius: 50%; }
  .dot.g { background: var(--green); }
  .bar { height: 5px; border-radius: 3px; background: var(--line); overflow: hidden; }
  .bar i { display: block; height: 100%; opacity: 0.8; border-radius: 3px; }
  .mono { font-family: var(--mono); }
  .m, .c, .tm { font-size: 11px; color: var(--muted); text-align: right; white-space: nowrap; }
  .stop { display: grid; place-items: center; width: 24px; height: 24px; border-radius: var(--r-sm); color: var(--muted); visibility: hidden; }
  .rw:hover .stop { visibility: visible; }
  .stop:hover { background: var(--surface-hover); color: var(--text-2); }
  .stop.armed { visibility: visible; background: var(--red-bg); color: var(--red); }
  .lk { display: grid; place-items: center; color: var(--muted); opacity: 0.6; }
</style>
