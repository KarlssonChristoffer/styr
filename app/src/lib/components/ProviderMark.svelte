<!-- The provider's mark in its own colour, drawn small and simple: Claude's burst, OpenAI's knot, DeepSeek's whale, Gemini's star. Brand colour is the one exception to status-only colour, since telling providers apart at a glance is the point.
     Used by ActivityList.svelte, ChildLine.svelte, Composer.svelte, ConnectionList.svelte and 12 more. -->
<script lang="ts">
  // The provider's mark in its own colour, drawn small and simple: Claude's burst, OpenAI's knot,
  // DeepSeek's whale, Gemini's star. Brand colour is the one exception to status-only colour,
  // since telling providers apart at a glance is the point.
  import type { ProviderId } from "../providers.svelte";

  let { id, size = 15 }: { id: ProviderId; size?: number } = $props();
  const rays = Array.from({ length: 10 }, (_, i) => (i * 36 * Math.PI) / 180);
</script>

<svg class="mark {id}" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
  {#if id === "anthropic"}
    <g stroke="currentColor" stroke-width="2.6" stroke-linecap="round">
      {#each rays as a, i (i)}
        <line x1={12 + Math.cos(a) * 2.2} y1={12 + Math.sin(a) * 2.2} x2={12 + Math.cos(a) * (i % 2 ? 8.2 : 10)} y2={12 + Math.sin(a) * (i % 2 ? 8.2 : 10)} />
      {/each}
    </g>
  {:else if id === "openai"}
    <g fill="none" stroke="currentColor" stroke-width="1.8">
      {#each [0, 60, 120] as r (r)}
        <rect x="8" y="2.8" width="8" height="18.4" rx="4" transform="rotate({r} 12 12)" />
      {/each}
    </g>
  {:else if id === "deepseek"}
    <path fill="currentColor" d="M21.6 6.1c-.3-.1-.4.1-.6.3l-.2.2c-.6.6-1.2 1-2 .9-1.2 0-2.2.3-3.1 1.2-.2-1.1-.8-1.7-1.7-2.1-.5-.2-1-.4-1.3-.9-.2-.3-.3-.7-.4-1.1 0-.2-.1-.5-.4-.5s-.4.2-.5.4c-.4.8-.6 1.6-.6 2.5 0 1.9.8 3.4 2.4 4.5.2.1.2.3.2.4-.1.4-.3.7-.4 1.1-.1.2-.2.3-.4.2-.9-.4-1.6-.9-2.3-1.6-1.1-1.1-2.1-2.3-3.4-3.2-.3-.2-.6-.4-.9-.6-1.3-1.3.2-2.3.5-2.5.4-.1.1-.6-1-.6S3.4 5 3 5.2c-.2.1-.4.1-.6.1-1.2-.3-2.5-.4-3.8-.2C-1 5.8-2.5 7.6-2.4 10.3c.2 3.5 2.3 6.4 5.5 8 3.4 1.7 7.4 1.7 11 .9.5-.1 1-.3 1.4-.5 1.4.5 2.6.6 3.5.3.6-.2 1-.6.5-.9-.6-.3-1.4-.4-2.1-.6.9-.9 1.6-2 2-3.2.5-1.2.7-2.5.8-3.8.4-.5 1-.7 1.6-.6.3 0 .5-.1.5-.4.2-1-.5-2.4-.7-3.4z" transform="translate(1.6 0) scale(.86) translate(1.4 1.6)" />
  {:else if id === "google"}
    <path fill="currentColor" d="M12 2c.6 5.2 4.8 9.4 10 10-5.2.6-9.4 4.8-10 10-.6-5.2-4.8-9.4-10-10 5.2-.6 9.4-4.8 10-10z" />
  {:else}
    <path fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z" />
  {/if}
</svg>

<style>
  .mark { flex: none; display: block; }
  .anthropic { color: #d97757; }
  .openai { color: var(--text); }
  .deepseek { color: #5b7bff; }
  .google { color: #6b9cf5; }
  .other { color: var(--accent); }
</style>
