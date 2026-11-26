<!-- The panel above the status bar, as in VS Code: Terminal (shells on the page's machine, opened in its folder), Ports and Problems. Its height is dragged from the top edge and kept. The bar's Terminal item and Ctrl Shift § open and close it.
     Used by App.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // The panel above the status bar, as in VS Code: Terminal (shells on the page's machine, opened
  // in its folder), Ports and Problems. Its height is dragged from the top edge and kept. The bar's
  // Terminal item and Ctrl Shift § open and close it.
  import { slide } from "../pop";
  import { api } from "../api";
  import type { Problem } from "../bindings/Problem";
  import { ago } from "../format";
  import { hub } from "../state.svelte";
  import { bar, type PanelTab } from "../statusbar.svelte";
  import Icon from "./Icon.svelte";
  import PortList from "./PortList.svelte";
  import TerminalPane from "./TerminalPane.svelte";
  import { middleClose } from "../middleClose";

  const place = $derived(bar.place);
  const shells = $derived(bar.shells);
  const shown = $derived(shells.find((t) => t.id === bar.terminal) ?? shells.at(-1) ?? null);

  let problems = $state<Problem[] | null>(null);
  $effect(() => {
    if (bar.panel !== "problems") return;
    void hub.problemsTick;
    api.problems({ view: "needs_you", limit: 50 }).then((p) => (problems = p)).catch(() => (problems = []));
  });

  const tabs: { tab: PanelTab; name: string }[] = [
    { tab: "terminal", name: "Terminal" },
    { tab: "ports", name: "Ports" },
    { tab: "problems", name: "Problems" },
  ];
  const count = (tab: PanelTab) => (tab === "terminal" ? shells.length : tab === "ports" ? bar.openPorts.filter((p) => !p.is_hub).length : hub.problemsNeedYou.length);

  function drag(e: PointerEvent) {
    e.preventDefault();
    const start = e.clientY;
    const from = bar.height;
    const move = (m: PointerEvent) => bar.setHeight(from + (start - m.clientY));
    const up = () => {
      removeEventListener("pointermove", move);
      removeEventListener("pointerup", up);
    };
    addEventListener("pointermove", move);
    addEventListener("pointerup", up);
  }

  function openProblems() {
    hub.view = "problems";
    hub.selected = null;
  }
</script>

<section class="panel" in:slide|global={{ y: 16 }} out:slide|global={{ out: true, y: 16 }} style:height="{bar.height}px" aria-label="Panel">
  <div class="edge" role="separator" aria-label="Resize the panel" onpointerdown={drag} ondblclick={() => bar.setHeight(280)}></div>
  <div class="tabs">
    {#each tabs as t (t.tab)}
      <button class="tab" class:on={bar.panel === t.tab} onclick={() => (bar.panel = t.tab)}>
        {t.name}{#if count(t.tab)}<em class:amber={t.tab === "problems"}>{count(t.tab)}</em>{/if}
      </button>
    {/each}
    <span class="gap"></span>
    {#if bar.panel === "terminal"}
      {#each shells as t, i (t.id)}
        <span class="shell" class:on={shown?.id === t.id} use:middleClose={() => bar.closeTerminal(t.id)}>
          <button class="pick" onclick={() => (bar.terminal = t.id)} data-tip={t.command}>{t.machine} {i + 1}</button>
          <button class="x" aria-label="Close this terminal" data-tip="Close it" onclick={() => bar.closeTerminal(t.id)}><Icon name="x" size={10} /></button>
        </span>
      {/each}
      <button class="icon" aria-label="New terminal" data-tip="New terminal on {place.machine}{place.cwd ? `, in ${place.label}` : ''}" onclick={() => bar.newTerminal()}><Icon name="plus" size={13} /></button>
    {/if}
    <button class="icon" aria-label="Close the panel" data-tip="Close the panel (Ctrl Shift §)" onclick={() => (bar.panel = null)}><Icon name="x" size={13} /></button>
  </div>
  <div class="body">
    {#if bar.panel === "terminal"}
      {#if shown}
        {#key shown.id}
          <div class="term"><TerminalPane id={shown.id} input={bar.typeFirst[shown.id]} ontyped={() => { const { [shown.id]: _, ...rest } = bar.typeFirst; bar.typeFirst = rest; }} onexit={() => bar.forget(shown.id)} /></div>
        {/key}
      {:else}
        <p class="quiet">Opening a terminal on {place.machine}…</p>
      {/if}
    {:else if bar.panel === "ports"}
      <div class="scroll"><PortList ports={bar.openPorts} host={place.host} /></div>
    {:else if bar.panel === "problems"}
      <div class="scroll">
        {#each problems ?? [] as p (p.id)}
          <button class="prob" onclick={openProblems} data-tip="Open Problems">
            <Icon name="alert" size={12} /><span class="grow">{p.title}</span>
            <span class="dim">{p.count}× · {ago(p.last_seen)}</span>
          </button>
        {:else}
          <Quiet kind={problems ? "none" : "loading"} text={problems ? "Nothing needs you" : undefined} />
        {/each}
      </div>
    {/if}
  </div>
</section>

<style>
  .panel { position: fixed; left: 0; right: 0; bottom: var(--bar-h); z-index: 40; display: flex; flex-direction: column; background: var(--bg-sidebar); border-top: 1px solid var(--line-strong); }
  .edge { position: absolute; left: 0; right: 0; top: -3px; height: 6px; cursor: ns-resize; z-index: 1; }
  .tabs { flex: none; display: flex; align-items: center; gap: 2px; height: 32px; padding: 0 8px; border-bottom: 1px solid var(--line); }
  .tab { height: 24px; padding: 0 8px; border-radius: var(--r-sm); font-size: 12px; color: var(--muted); display: inline-flex; align-items: center; gap: 4px; }
  .tab:hover { color: var(--text-2); }
  .tab.on { color: var(--text); background: var(--surface-hover); }
  .tab em { font-style: normal; color: var(--text-3); }
  .tab em.amber { color: var(--amber); }
  .gap { flex: 1; }
  .shell { display: inline-flex; align-items: center; border-radius: var(--r-sm); }
  .shell.on { background: var(--surface-hover); }
  .pick { height: 22px; padding: 0 2px 0 8px; font-size: 12px; color: var(--muted); }
  .shell.on .pick { color: var(--text-2); }
  .x { width: 18px; height: 18px; display: grid; place-items: center; border-radius: var(--r-xs); color: var(--muted); }
  .x:hover { color: var(--text); }
  .icon { width: 24px; height: 24px; display: grid; place-items: center; border-radius: var(--r-sm); color: var(--muted); }
  .icon:hover { color: var(--text); background: var(--surface-hover); }
  .body { flex: 1; min-height: 0; display: flex; flex-direction: column; padding: 4px 8px 6px; }
  .term { flex: 1; min-height: 0; }
  .scroll { overflow: auto; }
  .prob { display: flex; align-items: center; gap: 8px; width: 100%; height: 26px; padding: 0 6px; border-radius: var(--r-sm); text-align: left; font-size: 13px; color: var(--text-2); }
  .prob:hover { background: var(--surface-hover); }
  .prob :global(svg) { color: var(--amber); flex: none; }
  .grow { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dim { color: var(--muted); font-size: 12px; flex: none; }
  .quiet { margin: 6px; color: var(--muted); font-size: 13px; }
</style>
