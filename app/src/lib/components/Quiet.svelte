<!-- One quiet line for "reading", "nothing here" and "needs a newer Hub", so the same state has the same words on every page (DESIGN.md, Copy). `text` names what is missing ("No keys") when plain "None" would not say it; leave the how-to out when the button is already in view.
     Used by AgentKeys.svelte, ApiConnections.svelte, BottomPanel.svelte, ChatList.svelte and 61 more. -->
<script lang="ts" module>
  /** The fixed words for a state with nothing to show. `loading`: a read in flight. `none`: the
   *  list is there and empty. `newer`: the Hub on the other side lacks the call. */
  export const QUIET = { loading: "Reading…", none: "None", newer: "Needs a newer Hub" } as const;
  export type QuietKind = keyof typeof QUIET;
</script>

<script lang="ts">
  // One quiet line for "reading", "nothing here" and "needs a newer Hub", so the same state has the
  // same words on every page (DESIGN.md, Copy). `text` names what is missing ("No keys") when
  // plain "None" would not say it; leave the how-to out when the button is already in view.
  let { kind = "none", text, pad = false }: { kind?: QuietKind; text?: string; pad?: boolean } = $props();
</script>

<p class="quiet" class:pad class:newer={kind === "newer"}>{text ?? QUIET[kind]}</p>

<style>
  .quiet { color: var(--muted); font-size: 13px; line-height: 1.5; margin: 6px 0; overflow-wrap: anywhere; }
  .quiet.newer { opacity: 0.7; }
  .quiet.pad { padding: 0 10px; }
</style>
