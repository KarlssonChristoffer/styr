<!-- The sign in front of a thread's status in the inbox: a ring in the state's colour with a mark inside, so the shape says what is up before the word is read. Working is the working grid (`Working.svelte`), as everywhere in Hub. Read and quiet is an empty grey ring.
     Used by ActivityList.svelte, SessionRow.svelte, StatusBar.svelte. -->
<script lang="ts">
  import Working from "./Working.svelte";
  // The sign in front of a thread's status in the inbox: a ring in the state's colour with a
  // mark inside, so the shape says what is up before the word is read. Working is the working grid
  // (`Working.svelte`), as everywhere in Hub. Read and quiet is an empty grey ring.
  export type Sign = "asks" | "stuck" | "failed" | "limited" | "terminal" | "working" | "partway" | "done" | "idle" | "asleep";
  let { kind, size = 12, seed = "" }: { kind: Sign; size?: number; seed?: string } = $props();
</script>

{#if kind === "working"}
  <Working {size} {seed} />
{:else}
  <svg class="sign {kind}" viewBox="0 0 14 14" width={size} height={size} aria-hidden="true">
    {#if kind === "asleep"}
      <path d="M11.6 8.6A4.6 4.6 0 0 1 5.4 2.4a4.6 4.6 0 1 0 6.2 6.2z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" />
    {:else if kind === "partway"}
      <circle cx="7" cy="7" r="5.2" fill="none" stroke="currentColor" stroke-width="1.5" />
      <path d="M7 3.4a3.6 3.6 0 0 0 0 7.2z" fill="currentColor" stroke="none" />
    {:else}
      <circle cx="7" cy="7" r={kind === "idle" ? 4.6 : 5.5} fill="none" stroke="currentColor" stroke-width="1.5" />
      <g fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
        {#if kind === "asks"}
          <path d="M5.4 5.7c0-1 .8-1.7 1.7-1.7s1.7.6 1.7 1.5c0 1.2-1.7 1.3-1.7 2.5" /><circle cx="7.1" cy="10.1" r=".7" fill="currentColor" stroke="none" />
        {:else if kind === "stuck"}
          <path d="M7 4v3.8" /><circle cx="7" cy="10" r=".75" fill="currentColor" stroke="none" />
        {:else if kind === "failed"}
          <path d="M5 5l4 4M9 5l-4 4" />
        {:else if kind === "limited"}
          <path d="M7 4.4V7l1.8 1.1" />
        {:else if kind === "terminal"}
          <path d="M4.7 5.2L6.5 7 4.7 8.8M7.6 9h2" />
        {:else if kind === "done"}
          <path d="M4.7 7.2l1.6 1.6 3-3.4" />
        {/if}
      </g>
    {/if}
  </svg>
{/if}

<style>
  .sign { flex: none; display: block; }
  .sign.asks { color: var(--amber); }
  .sign.stuck, .sign.failed { color: var(--red); }
  .sign.limited { color: var(--amber); opacity: 0.75; }
  .sign.terminal, .sign.done, .sign.partway { color: var(--accent); }
  .sign.idle { color: var(--muted); }
  .sign.asleep { color: var(--accent); }
</style>
