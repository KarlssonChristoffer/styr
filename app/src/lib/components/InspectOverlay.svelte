<!-- Inspect mode over the page: a frame and a label on the element under the pointer, the marks with their numbers, and the note box at the element being written on. While it is on the page does nothing when clicked; a click picks the element instead. Esc leaves the mode.
     Used by App.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  // Inspect mode over the page: a frame and a label on the element under the pointer, the marks
  // with their numbers, and the note box at the element being written on. While it is on the page
  // does nothing when clicked; a click picks the element instead. Esc leaves the mode.
  import { onMount } from "svelte";
  import { inspect } from "../inspect.svelte";
  import { inUI, labelOf } from "../inspectInfo";
  import { magnified } from "../inspectSend";

  type Frame = { x: number; y: number; w: number; h: number };
  const rectOf = (el: Element): Frame => {
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, w: r.width, h: r.height };
  };
  const same = (a: Frame[], b: Frame[]) => a.length === b.length && a.every((f, i) => f.x === b[i].x && f.y === b[i].y && f.w === b[i].w && f.h === b[i].h);

  let hoverFrame = $state<Frame | null>(null);
  let hoverLabel = $state("");
  let frames = $state<Frame[]>([]);
  let noteBox = $state<HTMLTextAreaElement>();

  // Frames follow the page: scrolling, resizing and a view that changes under a mark.
  onMount(() => {
    let raf = 0;
    const loop = () => {
      const marks = inspect.marks;
      const next = marks.map((m) => (m.el.isConnected ? rectOf(m.el) : { x: 0, y: 0, w: 0, h: 0 }));
      if (!same(next, frames)) frames = next;
      const h = inspect.hover;
      if (h?.isConnected) {
        const f = rectOf(h);
        if (!hoverFrame || !same([hoverFrame], [f])) {
          hoverFrame = f;
          hoverLabel = labelOf(h);
        }
      } else if (hoverFrame) hoverFrame = null;
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    // The tree reads the page itself, so it is told now and then that the page may have changed.
    const timer = poll(() => inspect.rev++, 1000);
    return () => {
      cancelAnimationFrame(raf);
      timer();
    };
  });

  /** What the page would have got: every pointer event is held back from it, so the page does not act on a pick. */
  const hold = (e: Event) => {
    if (inUI(e.target as Element)) return;
    e.preventDefault();
    e.stopPropagation();
  };
  function move(e: MouseEvent) {
    const t = e.target as Element;
    inspect.pointer = { x: e.clientX, y: e.clientY };
    inspect.hover = inUI(t) || t === document.documentElement || t === document.body ? null : t;
  }
  function click(e: MouseEvent) {
    const t = e.target as Element;
    if (inUI(t)) return;
    hold(e);
    if (t === document.documentElement || t === document.body) return;
    inspect.pick(t, e.shiftKey);
  }
  function key(e: KeyboardEvent) {
    if (e.key.toLowerCase() === "z" && !e.ctrlKey && !e.metaKey && !e.altKey && !(e.target as Element)?.closest?.("input, textarea, [contenteditable]")) {
      e.preventDefault();
      inspect.magnify = !inspect.magnify;
      return;
    }
    if (e.key !== "Escape" || e.defaultPrevented) return;
    e.preventDefault();
    e.stopPropagation();
    if (inspect.sending) inspect.sending = false;
    else if (inspect.editing != null) inspect.editing = null;
    else inspect.stop();
  }
  onMount(() => {
    const opts = { capture: true } as const;
    const held = ["mousedown", "mouseup", "pointerdown", "pointerup", "dblclick", "contextmenu", "auxclick", "dragstart"];
    for (const n of held) window.addEventListener(n, hold, opts);
    window.addEventListener("mousemove", move, opts);
    window.addEventListener("click", click, opts);
    window.addEventListener("keydown", key, opts);
    document.addEventListener("mouseleave", () => (inspect.hover = null));
    return () => {
      for (const n of held) window.removeEventListener(n, hold, opts);
      window.removeEventListener("mousemove", move, opts);
      window.removeEventListener("click", click, opts);
      window.removeEventListener("keydown", key, opts);
    };
  });

  const editing = $derived(inspect.marks.findIndex((m) => m.id === inspect.editing));
  const note = $derived(editing >= 0 ? inspect.marks[editing] : null);
  const noteAt = $derived.by(() => {
    const f = editing >= 0 ? frames[editing] : null;
    if (!f) return null;
    const w = 320;
    const below = f.y + f.h + 8;
    const top = below + 150 > innerHeight ? Math.max(8, f.y - 150) : below;
    return { left: Math.min(Math.max(8, f.x), innerWidth - w - 8), top, w };
  });
  // The box opens a frame after the pick (it waits for the element's frame), so focus follows the box itself.
  $effect(() => {
    noteBox?.focus();
  });
  // The magnifier: the element under the pointer drawn larger, a moment after the pointer rests on it.
  let lens = $state<{ src: string; w: number; h: number } | null>(null);
  $effect(() => {
    const el = inspect.magnify ? inspect.hover : null;
    lens = null;
    if (!el) return;
    let stale = false;
    const t = setTimeout(async () => {
      const got = await magnified(el);
      if (!stale && got) lens = got;
    }, 140);
    return () => {
      stale = true;
      clearTimeout(t);
    };
  });
  const lensAt = $derived.by(() => {
    if (!lens) return null;
    const { x, y } = inspect.pointer;
    const left = x + 24 + lens.w > innerWidth - 8 ? Math.max(8, x - 24 - lens.w) : x + 24;
    const top = Math.min(Math.max(8, y - lens.h / 2), innerHeight - lens.h - 8);
    return { left, top };
  });
  const flagLeft = (f: Frame) => Math.max(0, f.x);
</script>

<div class="layer" data-inspect-ui>
  {#if hoverFrame && !inspect.marks.some((m) => m.el === inspect.hover)}
    <div class="frame hover" style:left="{hoverFrame.x}px" style:top="{hoverFrame.y}px" style:width="{hoverFrame.w}px" style:height="{hoverFrame.h}px"></div>
    <div class="label" style:left="{flagLeft(hoverFrame)}px" style:top="{hoverFrame.y < 26 ? hoverFrame.y + hoverFrame.h + 4 : hoverFrame.y - 22}px">{hoverLabel}<span>click to comment</span></div>
  {/if}
  {#each inspect.marks as m, i (m.id)}
    {@const f = frames[i]}
    {#if f && f.w + f.h > 0}
      <div class="frame mark" class:open={m.id === inspect.editing} style:left="{f.x}px" style:top="{f.y}px" style:width="{f.w}px" style:height="{f.h}px"></div>
      <div class="num" style:left="{Math.max(2, f.x - 7)}px" style:top="{Math.max(2, f.y - 9)}px">{inspect.tag(m)}</div>
    {/if}
  {/each}
  {#if lens && lensAt}
    <div class="lens" style:left="{lensAt.left}px" style:top="{lensAt.top}px" style:width="{lens.w}px" style:height="{lens.h}px">
      <img alt="" src={lens.src} width={lens.w} height={lens.h} />
    </div>
  {/if}
  {#if note && noteAt}
    {#key note.id}
    <div class="note" style:left="{noteAt.left}px" style:top="{noteAt.top}px" style:width="{noteAt.w}px">
      <div class="head"><b>{inspect.tag(note)}</b> {note.info.name || note.info.short}{#if note.group} <span class="g">group {note.group}</span>{/if}</div>
      <textarea bind:this={noteBox} rows="3" placeholder="What is wrong here?" value={note.note}
        oninput={(e) => inspect.setNote(note.id, e.currentTarget.value)}
        onkeydown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); inspect.editing = null; } }}></textarea>
      <div class="row">
        <span class="hint">Shift-click another element to group</span>
        <button class="ghost" onclick={() => inspect.remove(note.id)}>Remove</button>
        <button class="done" onclick={() => (inspect.editing = null)}>Done</button>
      </div>
    </div>
    {/key}
  {/if}
</div>

<style>
  .layer { position: fixed; inset: 0; z-index: 9000; pointer-events: none; font: 12px/1.4 var(--font); }
  .frame { position: absolute; box-sizing: border-box; border-radius: var(--r-sm); }
  .frame.hover { border: 1.5px dashed var(--accent); background: rgba(165, 171, 238, 0.08); }
  .frame.mark { border: 1.5px solid var(--accent); background: var(--accent-bg); }
  .frame.mark.open { box-shadow: 0 0 0 3px var(--accent-bg); }
  .label { position: absolute; background: var(--accent); color: var(--bg); padding: 1px 8px; border-radius: var(--r-xs); font-size: 11px; font-weight: 600; white-space: nowrap; }
  .label span { font-weight: 400; margin-left: 8px; opacity: 0.75; }
  .num { position: absolute; min-width: 18px; height: 18px; padding: 0 4px; box-sizing: border-box; border-radius: 999px; background: var(--accent); color: var(--bg); font-size: 11px; font-weight: 700; display: grid; place-items: center; }
  .note { position: absolute; pointer-events: auto; background: var(--surface); box-shadow: inset 0 0 0 1px var(--line-strong), 0 16px 40px rgba(0, 0, 0, 0.45); border-radius: var(--r-lg); padding: 10px; display: flex; flex-direction: column; gap: 8px; }
  .lens { position: absolute; box-sizing: border-box; background: var(--bg); border-radius: var(--r-lg); box-shadow: inset 0 0 0 1px var(--line-strong), 0 16px 40px rgba(0, 0, 0, 0.5); overflow: hidden; }
  .lens img { display: block; }
  .head { color: var(--muted); font-size: 12px; }
  .head b { color: var(--accent); margin-right: 4px; }
  .head .g { margin-left: 6px; }
  textarea { resize: none; background: var(--bg); border: 1px solid var(--line-strong); color: var(--text); border-radius: var(--r-md); padding: 8px 8px; font: 13px/1.5 var(--font); outline: none; }
  textarea:focus { border-color: var(--accent); }
  .row { display: flex; align-items: center; gap: 6px; }
  .hint { flex: 1; min-width: 0; color: var(--muted); font-size: 11px; }
  .ghost { color: var(--text-3); padding: 4px 8px; border-radius: var(--r-md); }
  .ghost:hover { background: var(--surface-hover); color: var(--text); }
  .done { background: var(--accent); color: var(--bg); border-radius: var(--r-md); padding: 4px 11px; font-weight: 600; }
</style>
