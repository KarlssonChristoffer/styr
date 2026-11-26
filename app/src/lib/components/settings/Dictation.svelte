<!-- Settings page: dictation. Used by Settings.svelte. -->
<script lang="ts">
  import { sc, edit } from "./core.svelte";
  import Quiet from "../Quiet.svelte";
  import { voiceApi } from "../../voice";
  import Switch from "../Switch.svelte";

  const saved = $derived(sc.core?.saved);

  let teaching = $state("");

  async function teach(key: string) {
    teaching = `Switch to Discord now: ${key} is pressed in 3 seconds…`;
    try {
      const res = await voiceApi.teach(key);
      teaching = res.ok ? `${key} pressed. Stop recording in Discord if it still listens.` : "Hub could not press the key.";
    } catch {
      teaching = "Hub could not press the key.";
    }
    setTimeout(() => (teaching = ""), 6000);
  }
</script>

<h1>Dictation</h1>
<p class="lead">Ctrl D or the microphone. Runs on this computer.</p>
{#if saved}
  <section>
    <div class="line">
      <div class="what"><div class="name">Dictation language</div><div class="hint">The language you mostly speak.</div></div>
      <div class="control seg">
        {#each [["sv", "Svenska"], ["en", "English"]] as [code, label] (code)}
          <button class:active={(saved.voice_language === "en" ? "en" : "sv") === code} onclick={() => edit("voice.language", code)}>{label}</button>
        {/each}
      </div>
    </div>
    <div class="line">
      <div class="what"><div class="name">Tidy what you said</div><div class="hint">Fixes punctuation and misheard words.</div></div>
      <div class="control"><Switch label="Tidy what you said" on={saved.voice_polish} onchange={(v) => edit("voice.polish", v)} /></div>
    </div>
    <div class="line">
      <div class="what">
        <div class="name">Mute voice chat while dictating</div>
        <div class="hint">
          Hub holds this key down while you dictate. In Discord, add a Push to Mute keybind, press Record, then Teach Discord here and switch back within 3 seconds.
          {#if teaching}<span class="teach-note">{teaching}</span>{/if}
        </div>
      </div>
      <div class="control">
        <div class="seg">
          {#each [[null, "Off"], ["F13", "F13"], ["F14", "F14"], ["F15", "F15"]] as [key, label] (label)}
            <button class:active={(saved.voice_mute_key ?? null) === key} onclick={() => edit("voice.mute_key", key)}>{label}</button>
          {/each}
        </div>
        <button class="btn" disabled={!saved.voice_mute_key || !!teaching} onclick={() => teach(saved.voice_mute_key!)}>Teach Discord</button>
      </div>
    </div>
  </section>
{:else}
  <section><Quiet kind="loading" /></section>
{/if}

<style>
  /* @kit settings */
</style>
