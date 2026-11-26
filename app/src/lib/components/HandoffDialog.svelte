<!-- "Hand to Knegarn" and the like: the thread's engine drafts a brief from the conversation with the tool's own guide, you read and change it, and "Hand over" is the approval. Nothing reaches the tool before that press. What the tool said comes back as a note in the thread.
     Used by App.svelte. -->
<script lang="ts">
  import { modal } from "../pop";
  // "Hand to Knegarn" and the like: the thread's engine drafts a brief from the conversation with
  // the tool's own guide, you read and change it, and "Hand over" is the approval. Nothing reaches
  // the tool before that press. What the tool said comes back as a note in the thread.
  import { api } from "../api";
  import type { ToolAsk } from "../bindings/ToolAsk";
  import { handing, type Taker } from "../ownTools.svelte";
  import { hub } from "../state.svelte";
  import Switch from "./Switch.svelte";

  let { thread, taker }: { thread: string; taker: Taker } = $props();

  let title = $state("");
  let brief = $state("");
  let drafting = $state(true);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let said = $state<string | null>(null);
  // "Let it build": without it the tool only tries (Knegarn's dry run builds nothing).
  let build = $state(true);
  // What the tool asked before it takes the work, such as how the repository runs its tests.
  let asks = $state<ToolAsk[]>([]);
  let answers = $state<Record<string, string>>({});

  const close = () => (handing.to = null);

  $effect(() => {
    api.draftHandoff(thread, taker.folder).then(
      (d) => {
        title = d.title;
        brief = d.brief;
        drafting = false;
      },
      (e) => {
        error = e instanceof Error ? e.message : String(e);
        drafting = false;
      },
    );
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      const ran = await api.handOff(thread, { folder: taker.folder, title: title.trim(), brief, build: taker.canBuild && build, answers });
      if (ran.ok) close();
      else {
        said = ran.output || "It refused without saying why.";
        if (ran.asks.length) {
          asks = ran.asks;
          for (const a of ran.asks) answers[a.name] ??= "";
        }
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function keys(e: KeyboardEvent) {
    if (e.key === "Escape" && !busy) close();
  }
</script>

<svelte:window onkeydown={keys} />

<div class="backdrop" role="presentation" in:modal|global out:modal|global={{ out: true }} onmousedown={(e) => e.target === e.currentTarget && !busy && close()}>
  <form class="dialog" onsubmit={submit}>
    <h2>{taker.label}</h2>
    {#if drafting}
      <p class="hint">Drafting the brief from this thread…</p>
    {:else}
      <input class="title" bind:value={title} placeholder="What the work is called" spellcheck="false" />
      <textarea class="brief mono" bind:value={brief} spellcheck="false" rows="18"></textarea>
      <p class="hint">{taker.about ?? `${taker.name} gets this text and nothing else.`}</p>
      {#if taker.canBuild}
        <div class="row">
          <span class="grow"><span class="label">Let it build</span><span class="sub">Off: {taker.name} only tries and builds nothing.</span></span>
          <Switch on={build} label="Let it build" onchange={(v) => (build = v)} />
        </div>
      {/if}
      {#each asks as ask (ask.name)}
        <label class="ask">
          <span class="label">{ask.label}</span>
          <input class="mono" bind:value={answers[ask.name]} placeholder={ask.hint ?? ""} spellcheck="false" />
        </label>
      {/each}
    {/if}
    {#if said}<pre class="said mono">{said}</pre>{/if}
    {#if error}<p class="error">{error}</p>{/if}
    <div class="actions">
      <button type="button" class="btn" onclick={close} disabled={busy}>Cancel</button>
      <button type="submit" class="btn primary" disabled={drafting || busy || !title.trim() || !brief.trim() || !hub.connected}>
        {busy ? "Handing over…" : asks.length ? "Add and hand over" : "Hand over"}
      </button>
    </div>
  </form>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.35); z-index: 50; display: flex; justify-content: center; align-items: flex-start; padding-top: 8vh; }
  .dialog { width: min(720px, calc(100vw - 32px)); background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.5); padding: 18px 20px 16px; }
  h2 { color: var(--text); font-size: 15px; font-weight: 500; margin: 0 0 12px; }
  .title { width: 100%; background: none; border: 0; outline: none; font-size: 16px; color: var(--text); padding: 2px 0 8px; }
  .title::placeholder { color: var(--muted); }
  .brief { width: 100%; resize: vertical; max-height: 60vh; background: var(--bg); border: 1px solid var(--line); border-radius: var(--r-md); outline: none; padding: 8px 10px; font-size: 13px; line-height: 1.5; color: var(--text-2); }
  .brief:focus { border-color: var(--line-strong); }
  .hint { color: var(--muted); font-size: 13px; margin: 8px 0 0; line-height: 1.5; }
  .said { color: var(--text-3); font-size: 12px; background: var(--bg); border-radius: var(--r-md); padding: 8px 10px; margin: 10px 0 0; white-space: pre-wrap; max-height: 160px; overflow-y: auto; }
  .error { color: var(--red); font-size: 13px; margin: 10px 0 0; }
  .row { display: flex; align-items: center; gap: 12px; margin-top: 12px; }
  .grow { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .label { color: var(--text-2); font-size: 13px; }
  .sub { color: var(--muted); font-size: 12px; }
  .ask { display: flex; flex-direction: column; gap: 4px; margin-top: 12px; }
  .ask input { background: var(--bg); border: 1px solid var(--line); border-radius: var(--r-md); outline: none; padding: 6px 10px; font-size: 13px; color: var(--text); }
  .ask input:focus { border-color: var(--line-strong); }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
</style>
