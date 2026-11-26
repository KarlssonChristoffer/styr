<!-- Picks a project's icon: one of Hub's line icons in one of its colours, or a picture of your own, shrunk to a small square. "Automatic" goes back to what Hub picks from the name.
     Used by App.svelte. -->
<script lang="ts">
  import { pop } from "../popover";
  // Picks a project's icon: one of Hub's line icons in one of its colours, or a picture of your
  // own, shrunk to a small square. "Automatic" goes back to what Hub picks from the name.
  import { autoLook, iconPicture, looks, PROJECT_COLORS, PROJECT_ICONS, type ProjectColor, type ProjectIcon } from "../looks.svelte";

  const at = $derived(looks.picking);
  const own = $derived(at ? looks.saved.projects[at.key] : undefined);
  const auto = $derived(at ? autoLook(at.name) : null);
  const icon = $derived((own?.icon as ProjectIcon | undefined) ?? auto?.icon);
  const color = $derived((own?.color as ProjectColor | undefined) ?? auto?.color);
  let error = $state("");
  let file: HTMLInputElement | undefined = $state();
  let box: HTMLDivElement | undefined = $state();

  // Kept inside the window, also when opened near the bottom or right edge.
  const pos = $derived.by(() => {
    if (!at) return { x: 0, y: 0 };
    const w = 296, h = 330;
    return { x: Math.max(8, Math.min(at.x, innerWidth - w - 8)), y: Math.max(8, Math.min(at.y, innerHeight - h - 8)) };
  });

  async function save(patch: { icon?: string | null; color?: string | null; image?: string | null }) {
    if (!at) return;
    error = "";
    const next = { icon: own?.icon ?? null, color: own?.color ?? null, image: own?.image ?? null, ...patch };
    try {
      await looks.setProject(at.key, next);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function upload(e: Event) {
    const f = (e.currentTarget as HTMLInputElement).files?.[0];
    (e.currentTarget as HTMLInputElement).value = "";
    if (!f) return;
    try {
      await save({ image: await iconPicture(f) });
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function close() {
    looks.picking = null;
    error = "";
  }

  function outside(e: PointerEvent) {
    if (at && box && !box.contains(e.target as Node)) close();
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={(e) => at && e.key === "Escape" && close()} />

{#if at && icon && color}
  <div transition:pop|global class="picker" bind:this={box} style:left="{pos.x}px" style:top="{pos.y}px" role="dialog" aria-label="Icon for {at.name}">
    <div class="head">
      {#if own?.image}
        <img src={own.image} alt="" width="28" height="28" />
      {:else}
        <svg class="big" viewBox="0 0 24 24" style:color="var(--p-{color})" aria-hidden="true"><path d={PROJECT_ICONS[icon]} /></svg>
      {/if}
      <span class="name ellipsis">{at.name}</span>
    </div>

    <div class="colors">
      {#each PROJECT_COLORS as c (c)}
        <button class="swatch" class:on={c === color && !own?.image} style:background="var(--p-{c})" data-tip={c} aria-label={c} onclick={() => save({ color: c, image: null })}></button>
      {/each}
    </div>

    <div class="icons">
      {#each Object.entries(PROJECT_ICONS) as [id, d] (id)}
        <button class="icon" class:on={id === icon && !own?.image} data-tip={id} aria-label={id} onclick={() => save({ icon: id, image: null })}>
          <svg viewBox="0 0 24 24" style:color="var(--p-{color})" aria-hidden="true"><path {d} /></svg>
        </button>
      {/each}
    </div>

    {#if error}<div class="error">{error}</div>{/if}
    <div class="foot">
      <button class="btn" onclick={() => file?.click()}>Upload picture…</button>
      <span class="hint" data-tip="Cropped square and shrunk to 96 px">Any picture</span>
      <button class="btn ghost" disabled={!own} onclick={() => save({ icon: null, color: null, image: null })}>Automatic</button>
    </div>
    <input bind:this={file} type="file" accept="image/png,image/jpeg,image/webp,image/gif,image/svg+xml,image/bmp" hidden onchange={upload} />
  </div>
{/if}

<style>
  .picker {
    position: fixed; z-index: 60; width: 296px; padding: 10px; border-radius: var(--r-md);
    background: var(--surface-2); border: 1px solid var(--line-strong); box-shadow: 0 12px 32px rgba(0, 0, 0, 0.35);
    display: flex; flex-direction: column; gap: 10px;
  }
  .head { display: flex; align-items: center; gap: 10px; padding: 2px 2px 0; }
  .head .name { color: var(--text); font-size: 14px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .head img { border-radius: 22%; object-fit: cover; }
  svg { fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
  .big { width: 28px; height: 28px; flex: none; }
  .colors { display: flex; gap: 6px; padding: 0 2px; }
  .swatch { width: 20px; height: 20px; border-radius: 50%; padding: 0; opacity: 0.85; }
  .swatch:hover { opacity: 1; }
  .swatch.on { box-shadow: 0 0 0 2px var(--surface-2), 0 0 0 3.5px var(--text-3); opacity: 1; }
  .icons { display: grid; grid-template-columns: repeat(8, 1fr); gap: 2px; }
  .icon { height: 32px; display: grid; place-items: center; border-radius: var(--r-sm); padding: 0; }
  .icon svg { width: 18px; height: 18px; }
  .icon:hover { background: var(--surface-hover); }
  .icon.on { background: var(--surface-hover); box-shadow: inset 0 0 0 1px var(--line-strong); }
  .foot { display: flex; align-items: center; gap: 8px; border-top: 1px solid var(--line); padding-top: 8px; }
  .foot .hint { flex: 1; color: var(--muted); font-size: 12px; }
  .error { color: var(--red); font-size: 12px; }
</style>
