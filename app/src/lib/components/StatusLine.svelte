<!-- The line above the composer while a thread works: the one live clock of the turn, and the ways to look in or stop it. What it does now is the transcript's live activity line and what it has done is the fold row ("So far · …"), each said once there. Colour only says state: the working grid (`WorkGrid.svelte`) plays while it works, an amber ring when it waits. In Minimal the chat's word row has the grid, so this line keeps only the clock.
     Used by ThreadView.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  // The line above the composer while a thread works: the one live clock of the turn, and the
  // ways to look in or stop it. What it does now is the transcript's live activity line and what it
  // has done is the fold row ("So far · …"), each said once there.
  // Colour only says state: the working grid (`WorkGrid.svelte`) plays while it works, an amber
  // ring when it waits. In Minimal the chat's word row has the grid, so this line keeps only the clock.
  import { duration, now as nowOf } from "../activity";
  import type { Item } from "../transcript";
  import Icon from "./Icon.svelte";
  import WorkGrid from "./WorkGrid.svelte";
  import { prefs } from "../prefs.svelte";
  import { workShape } from "../workGrid";
  import { stepsShown } from "../steps";

  let {
    items,
    waiting = false,
    onterminal,
    onstop,
    engine,
    word = false,
    detail = "normal",
    seed = "",
    sentAt = null,
  }: {
    /** When you last sent the thread work (`thread.sent_at`), the clock the activity list counts from.
     *  A message that waits behind an older turn is not in the fold's running turn, so without it this
     *  line would count from the old message while the list counts from the new one. */
    sentAt?: number | null;
    /** Picks the loop with "random", as the chat's own grid does: the thread. */
    seed?: string;
    /** The chat shows its work as one playful word (Minimal): that word says "Working", so this line only keeps the clock. */
    word?: boolean;
    /** The chat's level of detail: the steps card shows in all but Full and Minimal. */
    detail?: string;
    items: Item[];
    /** The engine's name, said while it starts. */
    engine?: string;
    /** The thread waits on you (a permission card), not on the engine. */
    waiting?: boolean;
    /** Opens the thread's terminal, shown while a command runs. */
    onterminal?: (command: string) => void;
    /** Stops the turn, as Esc in the composer does. */
    onstop?: () => void;
  } = $props();

  let tick = $state(Date.now());
  $effect(() => {
    const timer = poll(() => (tick = Date.now()), 1000);
    return () => timer();
  });

  const info = $derived(nowOf(items));
  // The steps card above holds the thread's one moving grid.
  const stepsUp = $derived(stepsShown(items, detail) != null);
  // The message is in but the engine has not taken it yet: it is starting (or loading the session).
  const starting = $derived.by(() => {
    const said = items.findLast((i) => i.kind === "user");
    return said?.kind === "user" && !!said.receipt && !said.receipt.taken && !said.receipt.done;
  });
  const command = $derived.by(() => {
    const last = items.at(-1);
    return last?.kind === "tool" && last.terminal && !last.until ? last : null;
  });
  const since = $derived(Math.max(info?.since ?? 0, sentAt ?? 0));
  const clock = (since: number) => duration(Math.max(0, tick - since));
</script>

{#if info}
  <div class="wrap">
    <div class="status" role="status" aria-live="off">
      {#if waiting}<span class="dot waiting_on_you"></span>{:else if !word && !stepsUp}<WorkGrid shape={workShape(seed, prefs.workIndicator)} size={14} />{/if}
      {#if !word || waiting || starting}<span class="what">{waiting ? "Waiting on you" : starting ? `Starting ${engine ?? "the engine"}` : "Working"}</span>{/if}
      <span class="time" title="Since {new Date(since).toLocaleTimeString([], { timeStyle: 'short' })}">{clock(since)}</span>
      <span class="end">
        {#if command && onterminal}
          <button class="link" onclick={() => onterminal(command.id)} title="Open the terminal (Ctrl §)"><Icon name="console" size={12} /><span class="word">Output</span></button>
        {/if}
        {#if onstop}<button class="link" onclick={onstop} title="Stop this turn (Esc in the composer)"><kbd>Esc</kbd><span class="word">stops</span></button>{/if}
      </span>
    </div>
  </div>
{/if}

<style>
  /* Narrow (a small window, big text or zoom): one line, the words dropped first. */
  .wrap { container: status / inline-size; }
  .status {
    display: flex; align-items: center; gap: 8px; min-width: 0;
    padding: 0 0 6px; color: var(--muted); font-size: 13px; line-height: 1.4;
  }
  .what { color: var(--text-3); }
  .time { font-variant-numeric: tabular-nums; white-space: nowrap; }
  .end { display: inline-flex; align-items: center; gap: 10px; margin-left: auto; white-space: nowrap; }
  .link { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: inherit; padding: 1px 6px; border-radius: var(--r-sm); }
  .link:hover { color: var(--text); background: var(--surface-hover); }
  @container status (max-width: 560px) {
    .word { display: none; }
  }
  @container status (max-width: 380px) {
    .what { display: none; }
  }
  kbd { font-family: var(--mono); font-size: 0.9em; padding: 0 4px; border: 1px solid var(--line-strong); border-radius: var(--r-xs); color: var(--text-3); }
</style>
