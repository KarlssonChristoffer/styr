<!-- Hub's on/off toggle, used instead of a browser checkbox. Controlled: the parent holds the state and gets `onchange`.
     Used by ApiConnections.svelte, CardMenu.svelte, ConnectionList.svelte, Customs.svelte and 12 more. -->
<script lang="ts">
  let { on, label, onchange, disabled = false }: { on: boolean; label: string; onchange: (on: boolean) => void; disabled?: boolean } = $props();
</script>

<button class="switch" class:on role="switch" aria-checked={on} aria-label={label} {disabled} onclick={() => onchange(!on)}>
  <span></span>
</button>

<style>
  .switch { width: 32px; height: 18px; border-radius: 999px; background: var(--line-strong); position: relative; flex: none; transition: background 0.12s var(--ease); }
  .switch span { position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%; background: var(--text-3); transition: transform 0.12s var(--ease), background 0.12s var(--ease); }
  .switch.on { background: var(--accent-bg); }
  .switch.on span { transform: translateX(14px); background: var(--accent); }
  /* The knob stretches 12% toward its travel while pressed (animation rule 5); the pressed state
     itself is not tweened beyond the 100ms. */
  .switch span { transform-origin: left center; }
  .switch.on span { transform-origin: right center; }
  @media (prefers-reduced-motion: no-preference) {
    .switch:not(:disabled):active span { scale: 1.12 1; transition-duration: 0.1s; }
    .switch span { transition-property: transform, background, scale; }
  }
  .switch:disabled { opacity: 0.5; cursor: default; }
</style>
