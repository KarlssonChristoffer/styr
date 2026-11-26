<!-- A widget on the Overview: a card an agent wrote on request (`widgets.rs`). While its thread builds it, a striped placeholder; then the page the thread showed, framed like any agent page (sandboxed, `pages.rs`). The page asks for Hub's numbers with `{hub: "data", want}` and gets the same read-only answers the other cards use; it can fetch nothing itself.
     Used by CardBody.svelte. -->
<script lang="ts">
  // A widget on the Overview: a card an agent wrote on request (`widgets.rs`). While its thread
  // builds it, a striped placeholder; then the page the thread showed, framed like any agent page
  // (sandboxed, `pages.rs`). The page asks for Hub's numbers with `{hub: "data", want}` and gets
  // the same read-only answers the other cards use; it can fetch nothing itself.
  import Working from "./Working.svelte";
  import { api, pageUrl } from "../api";
  import { folderName } from "../format";
  import { lines, loadLines, loadPulse, pulse, widgetOf } from "../overview.svelte";
  import { hub } from "../state.svelte";

  let { id }: { id: string } = $props();

  const widget = $derived(widgetOf(id));
  const tz = -new Date().getTimezoneOffset();

  let frame = $state<HTMLIFrameElement>();
  let height = $state<number | null>(null);
  const src = $derived(widget?.thread && widget.page ? pageUrl(widget.thread, widget.page) : null);

  const TOKENS = ["--surface", "--surface-2", "--surface-hover", "--line", "--line-strong", "--text", "--text-2", "--text-3", "--muted", "--accent", "--accent-bg", "--green", "--green-bg", "--amber", "--amber-bg", "--red", "--red-bg", "--radius", "--font", "--mono"];
  function theme() {
    const css = getComputedStyle(document.documentElement);
    // The card draws the background, so its colour and see-through show behind the page.
    return { ...Object.fromEntries(TOKENS.map((t) => [t, css.getPropertyValue(t).trim()])), "--bg": "transparent" };
  }

  // `pulse` and `lines` are `$state`, so their parts are proxies postMessage cannot clone:
  // they go out as snapshots.
  async function answer(want: string): Promise<unknown> {
    if (want === "pulse") {
      await loadPulse();
      const p = $state.snapshot(pulse.data);
      return p && { moments: p.moments, pulls: p.pulls, longest_thread: p.longest_thread };
    }
    if (want === "lines") {
      await loadLines();
      const l = $state.snapshot(lines.data);
      return l && { days: l.days };
    }
    if (want === "stats") {
      const s = await api.stats(30, tz);
      return {
        days: s.days,
        answers: s.answers,
        tokens: s.tokens,
        cost: s.cost,
        models: s.models.map((m) => ({ model: m.model, answers: m.answers, tokens: m.tokens, cost: m.cost })),
        projects: s.projects.map((p) => ({ name: p.name, tokens: p.tokens, cost: p.cost })),
      };
    }
    if (want === "threads") {
      return hub.threads
        .filter((t) => !t.archived)
        .map((t) => ({ title: t.title, status: t.status, project: folderName(t.cwd), updated: t.updated_at }));
    }
    if (want === "now") return { time: Date.now(), tz };
    return null;
  }

  let card = $state<HTMLElement>();
  /** A right-click inside the page stays in the page, so this opens the card's menu instead. */
  function menu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    card?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.left, clientY: r.bottom + 4 }));
  }

  function onMessage(e: MessageEvent) {
    if (!frame || e.source !== frame.contentWindow) return;
    const m = e.data;
    if (!m || typeof m !== "object") return;
    if (m.hub === "height" && typeof m.height === "number" && Number.isFinite(m.height)) height = Math.ceil(m.height);
    else if (m.hub === "ready") frame.contentWindow?.postMessage({ hub: "theme", vars: theme() }, "*");
    else if (m.hub === "data" && typeof m.want === "string") {
      const want = m.want;
      const to = frame.contentWindow;
      answer(want).then(
        (data) => to?.postMessage({ hub: "data", want, data }, "*"),
        () => to?.postMessage({ hub: "data", want, data: null }, "*"),
      );
    }
  }
</script>

<svelte:window onmessage={onMessage} />

<div class="widget" class:building={widget?.state === "building"} bind:this={card}>
  {#if src}<button class="more" onclick={menu} data-tip="Colour, size, links, ask AI to change it" aria-label="Card menu">⋯</button>{/if}
  {#if !widget}
    <h3>Widget</h3>
    <p class="muted">{"This widget is gone. Right-click to take it off the page."}</p>
  {:else if src}
    {#key src}
      <iframe bind:this={frame} {src} title={widget.title} sandbox="allow-scripts" style:height="{Math.max(height ?? 160, 60)}px"></iframe>
    {/key}
    {#if widget.state === "building"}
      <button class="redo" onclick={() => widget.thread && hub.open(widget.thread)} data-tip="Open the thread making the change"><Working size={11} />Changing…</button>
    {/if}
  {:else if widget.state === "building"}
    <h3><Working size={11} />Building · {widget.title}</h3>
    <p class="muted">An agent is writing this widget in the background, usually in a minute or three. It shows here by itself.</p>
    {#if widget.thread}<button class="link" onclick={() => hub.open(widget.thread!)}>Open the thread to watch or steer it</button>{/if}
  {:else}
    <h3>{widget.title}</h3>
    <p class="muted">The widget could not be made: {widget.error || "no page came back."}</p>
    {#if widget.thread}<button class="link" onclick={() => hub.open(widget.thread!)}>Open its thread</button>{/if}
  {/if}
</div>

<style>
  .widget { position: relative; min-height: 100%; }
  .widget.building:not(:has(iframe)) { margin: -14px; padding: 14px; border-radius: var(--r-lg); border: 1px dashed var(--line-strong); background: repeating-linear-gradient(135deg, var(--surface) 0 10px, var(--surface-2) 10px 20px); }
  h3 { display: flex; align-items: center; gap: 8px; color: var(--text); font-size: 13px; font-weight: 500; margin: 0 0 8px; }
  .muted { color: var(--muted); font-size: 12px; margin: 0 0 8px; }
  .link { color: var(--accent); font-size: 12px; padding: 0; }
  .link:hover { text-decoration: underline; }
  iframe { display: block; width: 100%; border: 0; background: transparent; color-scheme: dark; }
  .more { position: absolute; top: -8px; right: -6px; z-index: 1; width: 22px; height: 22px; border-radius: var(--r-sm); color: var(--muted); background: var(--surface-2); border: 1px solid var(--line-strong); opacity: 0; transition: opacity 0.12s var(--ease); line-height: 1; }
  .widget:hover .more, .more:focus-visible { opacity: 1; }
  .more:hover { color: var(--text); }
  .redo { position: absolute; top: -6px; right: 24px; display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--muted); background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: 999px; padding: 2px 8px; }
</style>
