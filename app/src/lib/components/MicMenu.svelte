<!-- The mic in the composer: dictate (Ctrl D), the live level while it listens, and beside it the menu for the microphone and for muting the voice chat. What dictation does is in `ComposerVoice` (composerVoice.svelte.ts); this is its button and menu.
     Used by Composer.svelte. -->
<script lang="ts">
  // The mic in the composer: dictate (Ctrl D), the live level while it listens, and beside it the
  // menu for the microphone and for muting the voice chat. What dictation does is in
  // `ComposerVoice` (composerVoice.svelte.ts); this is its button and menu.
  import type { ComposerVoice } from "../composerVoice.svelte";
  import { pop } from "../popover";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let { voice, open, ontoggle, onclose }: { voice: ComposerVoice; open: boolean; ontoggle: () => void; onclose: () => void } = $props();
</script>

<div class="menu-wrap mic-wrap">
  <button
    class="tool mic"
    class:recording={voice.state === "recording"}
    class:busy={voice.state === "transcribing"}
    data-tip={voice.note || (voice.state === "recording" ? "Stop dictating (Ctrl D)" : voice.state === "transcribing" ? "Transcribing…" : "Dictate (Ctrl D)")}
    aria-pressed={voice.state === "recording"}
    onclick={() => voice.toggle()}
  >
    <Icon name="mic" size={15} />
    {#if voice.state === "recording"}
      <span class="wave" aria-hidden="true">
        {#each voice.bars as b, i (i)}<i style:height="{3 + Math.round(b * 11)}px"></i>{/each}
      </span>
    {/if}
  </button>
  <button class="tool mic-pick" class:open data-tip="Choose microphone" aria-label="Choose microphone" onclick={() => { const opening = !open; ontoggle(); if (opening) voice.loadMics(); }}>
    <Icon name="down" size={12} />
  </button>
  {#if open}
    <div class="menu right mics" transition:pop|global role="menu">
      <div class="menu-title">Microphone</div>
      {#each [{ id: "", name: "System default" }, ...voice.mics] as m (m.id)}
        <button class="option row" class:active={m.id === voice.mic} onclick={() => { onclose(); voice.pickMic(m.id); }}>
          <span class="grow name">{m.name}</span>
          {#if m.id === voice.mic}<span class="tick"><Icon name="check" size={14} /></span>{/if}
        </button>
      {/each}
      <div class="divider"></div>
      {#if voice.muteKey}
        <button class="option row" data-tip="Holds {voice.muteKey}, Discord's Push to Mute key, while you dictate" aria-pressed={voice.muteChat} onclick={() => voice.toggleMuteChat()}>
          <span class="grow">Mute Discord while dictating</span><span class="switch" class:on={voice.muteChat}></span>
        </button>
      {:else}
        <button class="option row" data-tip="Pick a mute key and teach it to Discord" onclick={() => { onclose(); hub.openSettings("dictation"); }}>
          <span class="grow">Mute Discord while dictating</span><span class="set-up">Set up</span>
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .menu-wrap { position: relative; }
  .mic-wrap { display: inline-flex; align-items: center; }
  .tool {
    height: 28px; border-radius: var(--r-md); color: var(--muted); padding: 0 6px;
    display: inline-flex; align-items: center; gap: 4px; font-size: 13px;
  }
  /* The browser's own focus ring is a thick dark frame; a hairline is enough, and none round the
     green mic, whose colour already says it is on. */
  .tool:focus { outline: none; }
  .tool:focus-visible { box-shadow: inset 0 0 0 1px var(--line-strong); }
  .tool.mic.recording:focus-visible { box-shadow: none; }
  .tool:hover { background: var(--surface-hover); color: var(--text); }
  .mic-pick { padding: 0 2px; color: var(--muted); }
  .mic.recording, .mic.recording:hover { color: var(--bg); background: var(--green); }
  .mic.busy { color: var(--green); animation: pulse 1.2s var(--ease-move) infinite; }
  .wave { display: inline-flex; align-items: center; gap: 2px; height: 14px; }
  .wave i { width: 2px; border-radius: 1px; background: currentColor; transition: height 60ms linear; }
  .menu {
    position: absolute; z-index: 20; bottom: calc(100% + 6px); left: 0; min-width: 260px; max-width: 360px;
    background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg);
    padding: 4px; box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    max-height: 340px; overflow-y: auto;
  }
  .menu.right { left: auto; right: 0; }
  .menu-title { color: var(--muted); font-size: 12px; padding: 6px 8px 4px; }
  .option {
    display: flex; align-items: center; gap: 10px; width: 100%; text-align: left;
    padding: 4px 10px; border-radius: var(--r-md); color: var(--text-2); font-size: 14px; white-space: nowrap;
  }
  .option:hover, .option.active { background: var(--surface-hover); color: var(--text); }
  .grow { flex: 1; }
  .option .grow { display: inline-flex; align-items: center; gap: 8px; }
  .name { overflow: hidden; text-overflow: ellipsis; display: block; }
  .tick { color: var(--accent); display: grid; }
  .divider { height: 1px; background: var(--line-strong); margin: 4px 6px; }
  .set-up { color: var(--muted); font-size: 12px; }
  .switch { width: 18px; height: 10px; border-radius: 999px; background: var(--line-strong); position: relative; flex: none; }
  .switch::after { content: ""; position: absolute; top: 2px; left: 2px; width: 6px; height: 6px; border-radius: 50%; background: var(--muted); transition: left 0.12s var(--ease); }
  .switch.on { background: var(--accent-bg); }
  .switch.on::after { left: 10px; background: var(--accent); }
</style>
