<!-- The inbox simulator: fake threads on the core's `sim` engine, no AI. They work, ask for permission, ask a question, fail and finish on timers, so the Activity list can be tried.
     Used by Labs.svelte. -->
<script lang="ts">
  // The inbox simulator: fake threads on the core's `sim` engine, no AI. They work, ask for
  // permission, ask a question, fail and finish on timers, so the Activity list can be tried.
  import { api } from "../api";
  import { hub } from "../state.svelte";

  let acting = $state(false);
  let failed = $state<string | null>(null);

  const sims = $derived(hub.threads.filter((t) => t.engine === "sim"));
  const count = (s: string) => sims.filter((t) => t.status === s).length;
  const summary = $derived(
    [
      [count("running"), "working"],
      [count("waiting_on_you"), "asking you"],
      [count("error"), "failed"],
      [count("idle"), "idle"],
    ]
      .filter(([n]) => n)
      .map(([n, w]) => `${n} ${w}`)
      .join(" · "),
  );

  async function act(f: () => Promise<unknown>) {
    acting = true;
    try {
      await f();
      failed = null;
    } catch (e) {
      failed = e instanceof Error ? e.message : String(e);
    } finally {
      acting = false;
    }
  }

  const start = () =>
    act(async () => {
      await api.simStart();
      hub.switchRoom("work");
    });
</script>

<div class="sim">
  <p class="hint">
    Seven fake threads in a <b>Simulation</b> project, started a few seconds apart. Over about two minutes they work,
    ask for permission, ask you a question, fail and finish, so you can feel how the Activity list is to live with.
    Answer them, settle them, write to them: they reply with something dumb. No AI, no tokens.
  </p>
  <div class="actions">
    <button class="btn primary" disabled={acting} onclick={start}>{sims.length ? "Start more" : "Start"}</button>
    {#if sims.length}
      <button class="btn" disabled={acting} onclick={() => act(api.simClear)}>Remove simulated threads</button>
      <span class="state">{sims.length} simulated{summary ? ` · ${summary}` : ""}</span>
    {/if}
  </div>
  {#if failed}<pre class="error">{failed}</pre>{/if}
</div>

<style>
  .sim { border: 1px solid var(--line); border-radius: var(--r-md); padding: 12px 14px; display: flex; flex-direction: column; gap: 10px; }
  .hint { color: var(--text-3); margin: 0; max-width: 640px; }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
  .state { color: var(--muted); margin-left: 4px; }
  .error { color: var(--red); background: var(--red-bg); border-radius: var(--r-md); padding: 8px 10px; margin: 0; white-space: pre-wrap; font-size: 12px; }
</style>
