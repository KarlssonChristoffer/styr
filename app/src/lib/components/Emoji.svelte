<!-- One emoji: Unicode as text, or one of the user's own (`:name:`) as its picture.
     Used by EmojiPicker.svelte, Reactions.svelte. -->
<script lang="ts">
  // One emoji: Unicode as text, or one of the user's own (`:name:`) as its picture.
  import { emojis } from "../reactions.svelte";

  let { emoji, size = 15 }: { emoji: string; size?: number } = $props();
  const src = $derived(emojis.src(emoji));
</script>

{#if src}<img class="emoji" {src} alt={emoji} style:width="{size}px" style:height="{size}px" draggable="false" />{:else}<span class="emoji" style:font-size="{size}px" style:line-height="1">{emoji}</span>{/if}

<style>
  img.emoji { object-fit: contain; display: inline-block; vertical-align: middle; }
  span.emoji { font-family: "Apple Color Emoji", "Segoe UI Emoji", "Noto Color Emoji", sans-serif; display: inline-block; }
</style>
