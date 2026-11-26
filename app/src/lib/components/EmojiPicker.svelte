<!-- The reaction picker for one message: your most used emojis in a row, and "more" for all of them (search, groups) and your own, which you add here from a picture. Escape or a click elsewhere closes it.
     Used by Transcript.svelte. -->
<script lang="ts">
  import { pop } from "../popover";
  // The reaction picker for one message: your most used emojis in a row, and "more" for all of
  // them (search, groups) and your own, which you add here from a picture. Escape or a click
  // elsewhere closes it.
  import { tick } from "svelte";
  import { hub } from "../state.svelte";
  import { payload } from "../events";
  import { emojis, picker, react } from "../reactions.svelte";
  import Emoji from "./Emoji.svelte";
  import Icon from "./Icon.svelte";

  let { threadId }: { threadId: string } = $props();
  const at = $derived(picker.at?.thread === threadId ? picker.at : null);

  let full = $state(false);
  let query = $state("");
  let group = $state(0);
  let groups = $state<{ name: string; items: { e: string; n: string }[] }[]>([]);
  let box = $state<HTMLDivElement>();
  let search = $state<HTMLInputElement>();
  let file = $state<HTMLInputElement>();
  let busy = $state(false);
  let note = $state("");

  // A new message opens the quick row again.
  let last = "";
  $effect(() => {
    const key = at ? `${at.thread}:${at.seq}` : "";
    if (key !== last) {
      last = key;
      full = false;
      query = "";
      note = "";
    }
  });

  async function more() {
    full = true;
    group = mine.length ? 0 : 1;
    if (!groups.length) {
      const { GROUPS } = await import("../emojiData");
      groups = GROUPS.map(([name, text]) => ({ name, items: text.split("\n").map((l) => ({ e: l.slice(0, l.indexOf(" ")), n: l.slice(l.indexOf(" ") + 1) })) }));
    }
    await tick();
    search?.focus();
  }

  const mine = $derived(Object.keys(emojis.custom).sort());
  const q = $derived(query.trim().toLowerCase().replace(/^:|:$/g, ""));
  const found = $derived(
    q
      ? [
          ...mine.filter((n) => n.includes(q)).map((n) => ({ e: `:${n}:`, n })),
          ...groups.flatMap((g) => g.items.filter((i) => i.n.includes(q))),
        ].slice(0, 160)
      : [],
  );
  /** The tabs: your own first, when there are some. */
  const tabs = $derived([{ name: "Yours", glyph: "smilePlus" as const }, ...groups.map((g) => ({ name: g.name, glyph: null }))]);
  const shown = $derived(group === 0 ? mine.map((n) => ({ e: `:${n}:`, n })) : (groups[group - 1]?.items ?? []));

  function pick(emoji: string) {
    if (!at) return;
    react(at.thread, at.seq, emoji, !reactedBy(emoji));
    picker.at = null;
  }
  /** Picking one you already put there takes it away, as a chip does. */
  function reactedBy(emoji: string): boolean {
    const log = hub.events[threadId] ?? [];
    let on = false;
    for (const e of log) {
      const d = payload(e, "reaction");
      if (!d) continue;
      if (d.seq === at?.seq && d.emoji === emoji && d.by === "you") on = d.on !== false;
    }
    return on;
  }

  // A picture becomes an emoji: scaled down to at most 96 px, named after its file.
  async function addFile(f: File) {
    note = "";
    busy = true;
    try {
      const name = f.name.replace(/\.[^.]+$/, "").toLowerCase().replace(/[^a-z0-9_-]+/g, "_").replace(/^[^a-z0-9]+/, "").slice(0, 28) || "emoji";
      let unique = name;
      for (let i = 2; emojis.custom[unique]; i++) unique = `${name}_${i}`;
      let mime = "image/png";
      let data: string;
      if (f.type === "image/gif" && f.size <= 90 * 1024) {
        mime = "image/gif";
        data = await toBase64(f);
      } else {
        const bmp = await createImageBitmap(f);
        const k = Math.min(1, 96 / Math.max(bmp.width, bmp.height));
        const c = document.createElement("canvas");
        c.width = Math.max(1, Math.round(bmp.width * k));
        c.height = Math.max(1, Math.round(bmp.height * k));
        c.getContext("2d")!.drawImage(bmp, 0, 0, c.width, c.height);
        const blob = await new Promise<Blob | null>((ok) => c.toBlob(ok, "image/png"));
        if (!blob) throw new Error("Could not read that picture");
        data = await toBase64(blob);
      }
      await emojis.add(unique, mime, data);
      group = 0;
      note = `Added :${unique}:`;
    } catch (e) {
      note = e instanceof Error ? e.message : "Could not add it";
    } finally {
      busy = false;
      if (file) file.value = "";
    }
  }
  const toBase64 = (b: Blob) =>
    new Promise<string>((ok, no) => {
      const r = new FileReader();
      r.onload = () => ok(String(r.result).split(",")[1] ?? "");
      r.onerror = () => no(new Error("Could not read that picture"));
      r.readAsDataURL(b);
    });

  function key(e: KeyboardEvent) {
    if (e.key === "Escape" && at) {
      e.preventDefault();
      e.stopPropagation();
      picker.at = null;
    }
  }
  function outside(e: PointerEvent) {
    if (!at || !box) return;
    const t = e.target as HTMLElement;
    if (!box.contains(t) && !t.closest("[data-react-btn]")) picker.at = null;
  }
</script>

<svelte:window onkeydowncapture={key} onpointerdowncapture={outside} />

{#if at}
  <div transition:pop|global class="picker" class:full bind:this={box} role="dialog" aria-label="React" style:left="{at.x}px" style:top="{at.y}px" style:transform="translate({at.right ? '-100%' : '0'}, {at.up ? '-100%' : '0'})">
    <div class="quick">
      {#each emojis.quick as e (e)}
        <button class="em" data-tip={e.startsWith(":") ? e : undefined} onclick={() => pick(e)}><Emoji emoji={e} size={22} /></button>
      {/each}
      {#if !full}<button class="em more" data-tip="Any emoji, and your own" aria-label="More emojis" onclick={more}><Icon name="plus" size={16} /></button>{/if}
    </div>
    {#if full}
      <div class="find"><Icon name="search" size={13} /><input bind:this={search} bind:value={query} placeholder="Search emojis" spellcheck="false" autocomplete="off" /></div>
      {#if q}
        <div class="grid">
          {#each found as i (i.e + i.n)}<button class="em" data-tip={i.e.startsWith(":") ? i.e : i.n} onclick={() => pick(i.e)}><Emoji emoji={i.e} size={20} /></button>{/each}
          {#if !found.length}<div class="none">No emoji called “{query.trim()}”.</div>{/if}
        </div>
      {:else}
        <div class="tabs">
          {#each tabs as t, i (t.name)}
            <button class="tab" class:on={group === i} data-tip={t.name} aria-label={t.name} onclick={() => (group = i)}>
              {#if t.glyph}<Icon name={t.glyph} size={14} />{:else}<Emoji emoji={groups[i - 1]?.items[0]?.e ?? ""} size={15} />{/if}
            </button>
          {/each}
        </div>
        <div class="grid">
          {#if group === 0}
            <button class="em add" data-tip="Add a picture as an emoji" aria-label="Add your own emoji" disabled={busy} onclick={() => file?.click()}><Icon name="plus" size={16} /></button>
            {#each shown as i (i.e)}
              <span class="own">
                <button class="em" data-tip={i.e} onclick={() => pick(i.e)}><Emoji emoji={i.e} size={22} /></button>
                <button class="del" data-tip="Remove {i.e}" aria-label="Remove {i.e}" onclick={() => emojis.remove(i.n).catch((e) => hub.fail(e))}><Icon name="x" size={9} /></button>
              </span>
            {/each}
            {#if !mine.length}<div class="none">Add a picture and it becomes an emoji you can react with.</div>{/if}
          {:else}
            {#each shown as i (i.e)}<button class="em" data-tip={i.n} onclick={() => pick(i.e)}><Emoji emoji={i.e} size={20} /></button>{/each}
          {/if}
        </div>
        {#if note}<div class="note">{note}</div>{/if}
      {/if}
      <input class="file" type="file" accept="image/png,image/jpeg,image/webp,image/gif" bind:this={file} onchange={(e) => e.currentTarget.files?.[0] && addFile(e.currentTarget.files[0])} tabindex="-1" aria-hidden="true" />
    {/if}
  </div>
{/if}

<style>
  .picker {
    position: fixed; z-index: 60; background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg);
    padding: 6px; box-shadow: 0 8px 28px rgba(0, 0, 0, 0.38); display: flex; flex-direction: column; gap: 6px; font-size: 12px;
  }
  .picker.full { width: 316px; }
  .quick { display: flex; gap: 2px; }
  .em { display: grid; place-items: center; width: 34px; height: 34px; border-radius: var(--r-md); flex: none; }
  .em:hover { background: var(--surface-hover); }
  .em.more, .em.add { color: var(--muted); }
  .em.add { border: 1px dashed var(--line-strong); }
  .em.add:hover { color: var(--text); }
  .find { display: flex; align-items: center; gap: 8px; padding: 0 8px; height: 30px; border-radius: var(--r-md); background: var(--surface); color: var(--muted); }
  .find input { flex: 1; min-width: 0; background: none; border: 0; outline: 0; color: var(--text); font: inherit; font-size: 13px; }
  .tabs { display: flex; gap: 2px; border-bottom: 1px solid var(--line); padding-bottom: 4px; }
  .tab { display: grid; place-items: center; flex: 1; height: 28px; border-radius: var(--r-md); color: var(--muted); filter: grayscale(1); opacity: 0.8; }
  .tab:hover { background: var(--surface-hover); }
  .tab.on { background: var(--surface-hover); filter: none; opacity: 1; color: var(--text); }
  .grid { display: flex; flex-wrap: wrap; align-content: flex-start; gap: 1px; height: 214px; overflow-y: auto; padding-right: 2px; }
  .none { color: var(--muted); padding: 8px; line-height: 1.5; }
  .note { color: var(--muted); padding: 0 4px 2px; }
  .own { position: relative; display: inline-flex; }
  .del { position: absolute; top: 1px; right: 1px; display: none; place-items: center; width: 15px; height: 15px; border-radius: 50%; background: var(--line-strong); color: var(--text-2); }
  .own:hover .del, .own:focus-within .del { display: grid; }
  .file { display: none; }
</style>
