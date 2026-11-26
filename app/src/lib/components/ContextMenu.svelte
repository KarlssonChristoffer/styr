<!-- The one right-click menu. Arrow keys move, Right opens a submenu and Left closes it, Enter runs, Escape closes (a submenu first) and gives focus back.
     Used by App.svelte, TestWindow.svelte, ViewerWindow.svelte. -->
<script lang="ts">
  import { pop } from "../popover";
  // The one right-click menu. Arrow keys move, Right opens a submenu and Left closes it, Enter runs,
  // Escape closes (a submenu first) and gives focus back.
  import { tick } from "svelte";
  import { contextMenu, fallback, type MenuNode } from "../menu.svelte";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let el = $state<HTMLDivElement>();
  let pos = $state({ x: 0, y: 0 });
  /** The item asking "are you sure?" after a first click, by its place in the menus. */
  let arming = $state<string | null>(null);
  /** The submenus open, as the index chosen in each menu on the way: [3] is the fourth row's, [3, 1] its second row's. */
  let trail = $state<number[]>([]);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const open = $derived(contextMenu.open);
  const zoom = () => Number(document.documentElement.style.zoom) || 1;

  // Keep it on screen: flip left or up when it would run past the edge.
  $effect(() => {
    if (!open || !el) return;
    arming = null;
    trail = [];
    const { width, height } = el.getBoundingClientRect();
    const w = window.innerWidth / zoom(), h = window.innerHeight / zoom();
    pos = {
      x: open.x + width > w - 4 ? Math.max(4, open.x - width) : open.x,
      y: open.y + height > h - 4 ? Math.max(4, h - height - 4) : open.y,
    };
    el.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus({ preventScroll: true });
  });

  const key = (path: number[], i: number) => [...path, i].join(".");
  const isOpen = (path: number[], i: number) =>
    trail.length > path.length && trail[path.length] === i && path.every((v, k) => trail[k] === v);

  /** A submenu goes beside its row, on the side that has room, and up when it would run off the bottom. */
  function place(node: HTMLElement) {
    const z = zoom();
    const parent = node.parentElement?.closest<HTMLElement>(".menu");
    let left = parent?.dataset.dir === "left";
    const r = node.getBoundingClientRect();
    if (!left && r.right / z > window.innerWidth / z - 4) left = true;
    if (left) {
      node.style.left = "auto";
      node.style.right = "calc(100% + 5px)";
    }
    node.dataset.dir = left ? "left" : "right";
    const over = r.bottom / z - (window.innerHeight / z - 4);
    if (over > 0) node.style.top = `${-5 - over}px`;
  }

  function run(item: MenuNode, path: number[], i: number) {
    if (item.children) {
      trail = [...path, i];
      return;
    }
    if (item.confirm && arming !== key(path, i)) {
      arming = key(path, i);
      return;
    }
    contextMenu.close();
    // An action that fails says so, as the toast; a menu that closes on nothing reads as a bug.
    Promise.resolve()
      .then(item.run)
      .catch((e) => hub.fail(e));
  }

  /** Pointing at a row focuses it, and opens its submenu (or closes the ones beside it) after a beat,
   *  so a slanted move toward a submenu does not close it on the way. */
  function enter(e: PointerEvent, item: MenuNode, path: number[], i: number) {
    if (item.disabled) return;
    (e.currentTarget as HTMLButtonElement).focus({ preventScroll: true });
    clearTimeout(timer);
    timer = setTimeout(() => (trail = item.children ? [...path, i] : path.slice()), item.children ? 120 : 220);
  }

  const buttons = (panel: Element) => [...panel.querySelectorAll<HTMLButtonElement>(":scope > .slot > button:not(:disabled)")];

  async function keydown(e: KeyboardEvent) {
    if (!el) return;
    const active = document.activeElement as HTMLButtonElement | null;
    const panel = active?.closest<HTMLElement>("[role=menu]") ?? el;
    const list = buttons(panel);
    const at = active ? list.indexOf(active) : -1;
    const go = (i: number) => list[(i + list.length) % list.length]?.focus();
    const sub = panel !== el;
    if (e.key === "ArrowDown") go(at + 1);
    else if (e.key === "ArrowUp") go(at < 0 ? -1 : at - 1);
    else if (e.key === "Home") go(0);
    else if (e.key === "End") go(-1);
    else if (e.key === "ArrowRight" && active?.getAttribute("aria-haspopup")) {
      active.click();
      await tick();
      const inner = active.parentElement?.querySelector<HTMLElement>(":scope > .menu");
      if (inner) buttons(inner)[0]?.focus();
    } else if ((e.key === "ArrowLeft" || e.key === "Escape") && sub) {
      const slot = panel.parentElement;
      trail = trail.slice(0, -1);
      slot?.querySelector<HTMLButtonElement>(":scope > button")?.focus();
    } else if (e.key === "Escape" || e.key === "Tab") contextMenu.close();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  function outside(e: Event) {
    if (open && el && !e.composedPath().includes(el)) contextMenu.close();
  }
</script>

<svelte:window
  oncontextmenu={fallback}
  onpointerdowncapture={outside}
  onblur={() => open && contextMenu.close()}
  onresize={() => open && contextMenu.close()}
  onwheelcapture={outside}
/>

{#snippet panel(items: (MenuNode | null)[], path: number[])}
  {#each items as item, i (i)}
    {#if item === null}
      <div class="sep" role="separator"></div>
    {:else if item.heading}
      <div class="heading">{item.label}</div>
    {:else}
      <div class="slot">
        <button
          role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
          tabindex="-1"
          class:danger={item.danger}
          class:armed={arming === key(path, i)}
          class:open={isOpen(path, i)}
          disabled={item.disabled}
          aria-haspopup={item.children ? "menu" : undefined}
          aria-expanded={item.children ? isOpen(path, i) : undefined}
          aria-checked={item.checked === undefined ? undefined : item.checked}
          onclick={() => run(item, path, i)}
          onpointerenter={(e) => enter(e, item, path, i)}
        >
          {#if item.icon}<span class="icon"><Icon name={item.icon} size={15} /></span>{/if}
          <span class="label">{arming === key(path, i) ? item.confirm : item.label}</span>
          {#if item.hint && arming !== key(path, i)}<span class="hint">{item.hint}</span>{/if}
          {#if item.checked}<span class="tick"><Icon name="check" size={14} /></span>{/if}
          {#if item.children}<span class="more"><Icon name="chevron" size={12} /></span>{/if}
        </button>
        {#if item.children && isOpen(path, i)}
          <div class="menu sub" role="menu" transition:pop|global={{ origin: (n) => ((n as HTMLElement).dataset.dir === "left" ? "100% 0" : "0 0") }} tabindex="-1" use:place onpointerenter={() => clearTimeout(timer)}>
            {@render panel(item.children, [...path, i])}
          </div>
        {/if}
      </div>
    {/if}
  {/each}
{/snippet}

{#if open}
  <div
    class="menu"
    transition:pop|global
    role="menu"
    tabindex="-1"
    bind:this={el}
    style:left="{pos.x || open.x}px"
    style:top="{pos.y || open.y}px"
    onkeydown={keydown}
    oncontextmenu={(e) => e.preventDefault()}
  >
    {@render panel(open.items, [])}
  </div>
{/if}

<style>
  .menu {
    position: fixed; z-index: 100; min-width: 180px; max-width: 320px;
    background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 4px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.4); display: flex; flex-direction: column; outline: none;
  }
  /* A submenu hangs off its row: no fixed position of its own, so it follows the menu. */
  .menu.sub { position: absolute; left: calc(100% + 5px); top: -5px; min-width: 200px; z-index: 1; }
  .slot { position: relative; display: flex; flex-direction: column; }
  button {
    display: flex; align-items: center; gap: 10px; text-align: left;
    padding: 4px 10px; border-radius: var(--r-sm); font-size: 13px; color: var(--text-2); outline: none;
  }
  button:hover, button:focus-visible, button:focus, button.open { background: var(--surface-hover); color: var(--text); }
  button:disabled { color: var(--muted); background: none; }
  .icon { display: flex; flex: none; color: var(--muted); }
  button:focus .icon, button:hover .icon, button.open .icon { color: var(--text-2); }
  .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hint { color: var(--muted); font-size: 12px; white-space: nowrap; max-width: 140px; overflow: hidden; text-overflow: ellipsis; margin-left: 6px; }
  .tick { display: flex; flex: none; color: var(--accent); }
  .more { display: flex; flex: none; color: var(--muted); margin-right: -4px; }
  .danger, .danger:hover, .danger:focus, .danger .icon { color: var(--red); }
  .armed { background: var(--red-bg); }
  .sep { height: 1px; background: var(--line); margin: 4px 6px; }
  .heading { font-size: 12px; color: var(--muted); padding: 4px 10px 3px; }
</style>
