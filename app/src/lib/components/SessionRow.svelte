<!-- One agent session as a line: state icon, title, provider, memory and time, no words (they are on the hover). In a machine's card it is the short line; on the machine's page (`wide`) it also has its share of the memory, the CPU and a stop button.
     Used by MachineCard.svelte, MachineView.svelte. -->
<script lang="ts">
  // One agent session as a line: state icon, title, provider, memory and time, no words (they are
  // on the hover). In a machine's card it is the short line; on the machine's page (`wide`) it
  // also has its share of the memory, the CPU and a stop button.
  import { api } from "../api";
  import { bytes } from "../format";
  import { providerOf, harnessLabel } from "../providers.svelte";
  import { sessions, timeOf, type Session, type SessionState } from "../sessions.svelte";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import ProviderMark from "./ProviderMark.svelte";
  import StateSign, { type Sign } from "./StateSign.svelte";

  let { s, total = 0, wide = false }: { s: Session; total?: number; wide?: boolean } = $props();

  const SIGN: Record<SessionState, Sign> = { working: "working", asks: "asks", idle: "idle", lingering: "failed", asleep: "asleep" };
  const WORDS: Record<SessionState, string> = {
    working: "Working",
    asks: "Waiting on you",
    idle: "Idle",
    lingering: "Should be gone: its thread is settled or archived",
    asleep: "Asleep: waiting on you, wakes with your answer",
  };
  const t = $derived(timeOf(s, hub.now));
  const share = $derived(total ? Math.max(s.mem > 0 ? 2 : 0, Math.min(100, (s.mem / total) * 100)) : 0);
  const pct = (n: number) => `${n >= 10 ? Math.round(n) : n.toFixed(1)}%`;

  let busy = $state(false);
  /** A stop waiting for its second click, for a session in the middle of something. */
  let armed = $state(false);
  async function stop() {
    if ((s.state === "working" || s.state === "asks") && !armed) {
      armed = true;
      setTimeout(() => (armed = false), 4000);
      return;
    }
    busy = true;
    armed = false;
    try {
      await api.shutdown(s.id, s.state === "lingering", s.host);
      await sessions.load();
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = false;
    }
  }
</script>

<div role="presentation" class="sr" class:wide class:lingering={s.state === "lingering"} onclick={(e) => e.stopPropagation()}>
  <span class="sign" data-tip={WORDS[s.state]}><StateSign kind={SIGN[s.state]} size={15} /></span>
  {#if s.thread}
    <button class="title" data-tip="Open the thread" onclick={() => hub.open(s.id)}>{s.thread.title}</button>
  {:else}
    <span class="title gone" data-tip="The thread is gone, what it started still runs">A removed thread</span>
  {/if}
  <span class="agent" data-tip={s.thread ? harnessLabel(s.thread.engine) : ""}>{#if s.thread}<ProviderMark id={providerOf(s.thread.engine).id} size={13} />{/if}</span>
  {#if wide}
    <span class="bar" data-tip="Its share of the machine's memory"><i style:width="{share}%"></i></span>
  {/if}
  <span class="mem mono" data-tip={s.mem ? `${bytes(s.mem)} in ${s.processes} ${s.processes === 1 ? "process" : "processes"}` : ""}>{s.mem ? bytes(s.mem) : "—"}</span>
  {#if wide}<span class="cpu mono" data-tip="Share of the whole machine">{s.mem || s.cpu ? pct(s.cpu) : ""}</span>{/if}
  <span class="time mono {s.state}" data-tip={t.says}>{t.text}</span>
  {#if wide}
    {#if s.state === "asleep"}
      <span class="stop"></span>
    {:else}
      <button class="stop" class:armed disabled={busy}
        data-tip={armed ? "Click again: this cuts off what it is doing" : s.state === "lingering" ? "Stop it and everything it started" : "Stop the engine; the next message starts it again"}
        onclick={stop}><Icon name="stop" size={13} /></button>
    {/if}
  {/if}
</div>

<style>
  .sr { display: grid; grid-template-columns: 18px minmax(0, 1fr) 16px 58px 48px; gap: 10px; align-items: center; padding: 4px 0; font-size: 13px; cursor: default; }
  .sr.wide { grid-template-columns: 18px minmax(0, 1fr) 16px 84px 56px 42px 54px 26px; padding: 6px 8px; border-radius: var(--r-md); }
  .sr.wide:hover { background: var(--surface); }
  .sr.lingering { background: var(--red-bg); border-radius: var(--r-md); padding-inline: 6px; }
  .sign { display: grid; place-items: center; }
  .title { text-align: left; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; padding: 0; font-size: inherit; }
  .title:hover { text-decoration: underline; text-underline-offset: 3px; }
  .title.gone { color: var(--text-3); }
  .agent { display: grid; place-items: center; color: var(--text-3); }
  .mono { font-family: var(--mono); }
  .mem, .cpu { text-align: right; color: var(--muted); font-size: 11px; white-space: nowrap; }
  .bar { height: 5px; border-radius: 3px; background: var(--line); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--text-3); opacity: 0.6; border-radius: 3px; }
  .time { text-align: right; color: var(--text-3); font-size: 12px; white-space: nowrap; }
  .time.working { color: var(--green); }
  .time.asks { color: var(--amber); }
  .time.lingering { color: var(--red); }
  .time.asleep { color: var(--accent); }
  .stop { display: grid; place-items: center; width: 24px; height: 24px; border-radius: var(--r-sm); color: var(--muted); visibility: hidden; }
  .sr:hover .stop { visibility: visible; }
  .stop:hover { background: var(--surface-hover); color: var(--text-2); }
  .stop.armed { visibility: visible; background: var(--red-bg); color: var(--red); }
</style>
