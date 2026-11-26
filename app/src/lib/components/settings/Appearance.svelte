<!-- Settings page: appearance. Used by Settings.svelte. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { CARD_THEMES, PICKER_THEMES } from "../../cardSwitch";
  import { THEMES } from "../../themes";
  import { prefs, setPref } from "../../prefs.svelte";
  import { STEPS, setZoom, zoomLevel } from "../../zoom";
  import { resetScreen, screenNow } from "../../screens.svelte";
  import Field from "../Field.svelte";
  import Switch from "../Switch.svelte";
  import TypographySettings from "../TypographySettings.svelte";

  let zoom = $state(zoomLevel());

  function setCoord(key: "lat" | "lon", text: string, limit: number) {
    const n = Number(text.replace(",", "."));
    if (Number.isFinite(n) && Math.abs(n) <= limit) setPref("place", { ...prefs.place, [key]: n });
  }

  onMount(() => {
    const follow = (e: Event) => (zoom = (e as CustomEvent<number>).detail);
    window.addEventListener("hub-zoom", follow);
    return () => window.removeEventListener("hub-zoom", follow);
  });
</script>

<h1>Appearance</h1>
<h2>Look</h2>
<section>
  <div class="line">
    <div class="what"><div class="name">Theme</div></div>
  </div>
  <div class="themes" role="radiogroup" aria-label="Theme">
    {#each THEMES as th (th.id)}
      <button class="theme" class:active={prefs.theme === th.id} role="radio" aria-checked={prefs.theme === th.id} onclick={() => setPref("theme", th.id)}>
        <span class="prev" style="background: {th.bg}; border-color: color-mix(in srgb, {th.text} 18%, {th.bg});">
          <i class="bar" style="background: {th.surface}"></i>
          <i class="dot" style="background: {th.accent}"></i>
          <i class="ln" style="background: {th.text}"></i>
        </span>
        <span class="tname">{th.label}</span>
      </button>
    {/each}
  </div>
</section>
<TypographySettings />

<h2>Size</h2>
<section>
  <div class="line">
    <div class="what"><div class="name">Zoom</div></div>
    <div class="control">
      <button class="btn" disabled={zoom <= STEPS[0]} onclick={() => setZoom(STEPS[Math.max(0, STEPS.indexOf(zoom) - 1)])}>−</button>
      <span class="value mono">{Math.round(zoom * 100)}%</span>
      <button class="btn" disabled={zoom >= STEPS.at(-1)!} onclick={() => setZoom(STEPS[Math.min(STEPS.length - 1, STEPS.indexOf(zoom) + 1)])}>+</button>
      <button class="btn" disabled={zoom === 1} onclick={() => setZoom(1)}>Reset</button>
    </div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Text size</div><div class="hint">Small text grows most.</div></div>
    <div class="control seg">
      {#each [["default", "Default"], ["larger", "Larger"], ["large", "Large"], ["largest", "Largest"]] as [id, label] (id)}
        <button class:active={prefs.textSize === id} onclick={() => setPref("textSize", id as typeof prefs.textSize)}>{label}</button>
      {/each}
    </div>
  </div>
  {#if screenNow.known}
    <div class="line">
      <div class="what"><div class="name">This screen{#if screenNow.label} <span class="hint-inline">{screenNow.label}</span>{/if}</div><div class="hint">Kept per screen.</div></div>
      <div class="control"><button class="btn" onclick={resetScreen}>Use Hub's sizes here</button></div>
    </div>
  {/if}
</section>

<h2>More</h2>
<section>
  <div class="line">
    <div class="what"><div class="name">Your place</div><div class="hint">Latitude, longitude. Used for daylight colours.</div></div>
    <div class="control place">
      <Field value={prefs.place.name} placeholder="Stockholm" width={140} onsave={(v) => setPref("place", { ...prefs.place, name: v || "Here" })} />
      <Field value={String(prefs.place.lat)} placeholder="59.33" width={80} onsave={(v) => setCoord("lat", v, 90)} />
      <Field value={String(prefs.place.lon)} placeholder="18.07" width={80} onsave={(v) => setCoord("lon", v, 180)} />
    </div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Reduce motion</div></div>
    <div class="control"><Switch label="Reduce motion" on={prefs.reduceMotion} onchange={(v) => setPref("reduceMotion", v)} /></div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Limits card backdrop</div></div>
    <div class="control seg">
      {#each CARD_THEMES as [id, label] (id)}
        <button class:active={prefs.cardTheme === id} onclick={() => setPref("cardTheme", id)}>{label}</button>
      {/each}
    </div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Model picker colour</div></div>
    <div class="control seg">
      {#each PICKER_THEMES as [id, label] (id)}
        <button class:active={prefs.pickerTheme === id} onclick={() => setPref("pickerTheme", id)}>{label}</button>
      {/each}
    </div>
  </div>
</section>

<style>
  /* @kit settings */
</style>
