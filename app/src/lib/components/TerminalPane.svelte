<!-- A terminal inside Hub (`pty.rs` in the core): xterm.js draws what the core's pseudo-terminal prints and sends keys back. The terminal keeps running in the core when this is gone; showing it again draws its output so far first.
     Used by BottomPanel.svelte, MachineView.svelte, RunTerminal.svelte, ThreadPanel.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import { socketUrl } from "../home";
  import { findLinks } from "../terminal-links";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";

  // A terminal inside Hub (`pty.rs` in the core): xterm.js draws what the core's pseudo-terminal
  // prints and sends keys back. The terminal keeps running in the core when this is gone; showing
  // it again draws its output so far first.
  let { id, onexit, input, ontyped }: {
    id: string;
    onexit?: (code: number) => void;
    /** Typed once when the terminal first connects, such as a `cd` into a folder. */
    input?: string;
    ontyped?: () => void;
  } = $props();

  // The terminal is fitted to `box`, which has no padding of its own: the fit addon takes the
  // parent's computed height, which with border-box includes padding, so padding on the fitted
  // element made the terminal a row too tall and cut its last line off.
  let box: HTMLDivElement;
  let sendKeys: (data: string) => void = () => {};
  /** Types text into the terminal as if it was typed, for a command to run once the person has seen it. */
  export function type(text: string) {
    sendKeys(text);
  }
  let exit = $state<number | null>(null);
  let lost = $state(false);

  /** The theme's own colours, so the terminal sits in Hub like the rest. */
  function theme() {
    const css = getComputedStyle(document.documentElement);
    const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
    return {
      background: v("--bg-sidebar", "#151413"),
      foreground: v("--text", "#ece8e2"),
      cursor: v("--text-2", "#c9c3bb"),
      cursorAccent: v("--bg-sidebar", "#151413"),
      selectionBackground: v("--accent-bg", "rgba(165, 171, 238, 0.25)"),
      black: v("--surface-hover", "#2a2826"),
      brightBlack: v("--muted", "#958f88"),
      red: v("--red", "#e5806f"),
      brightRed: v("--red", "#e5806f"),
      green: v("--green", "#84c29a"),
      brightGreen: v("--green", "#84c29a"),
      yellow: v("--amber", "#ddaa5e"),
      brightYellow: v("--amber", "#ddaa5e"),
      blue: v("--p-blue", "#7fa6ee"),
      brightBlue: v("--p-blue", "#7fa6ee"),
      magenta: v("--p-violet", "#a99cf0"),
      brightMagenta: v("--p-pink", "#e095d6"),
      cyan: v("--p-sky", "#78c4e6"),
      brightCyan: v("--p-teal", "#6cc7b8"),
      white: v("--text-2", "#c9c3bb"),
      brightWhite: v("--text", "#ece8e2"),
    };
  }

  onMount(() => {
    const css = getComputedStyle(document.documentElement);
    const term = new Terminal({
      theme: theme(),
      fontFamily: css.getPropertyValue("--mono").trim() || "monospace",
      fontSize: Math.round(13 * (Number(css.getPropertyValue("--code-scale")) || 1) * 2) / 2,
      lineHeight: 1.15,
      cursorBlink: true,
      scrollback: 5000,
      allowProposedApi: false,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(box);

    // Links open in the system browser. Without this a click only focuses the terminal, and a link
    // wrapped over several rows (the Claude sign-in URL) was never one link at all.
    term.registerLinkProvider({
      provideLinks(y, done) {
        const buf = term.buffer.active;
        let top = y - 1;
        while (top > 0 && buf.getLine(top)?.isWrapped) top--;
        const rows: string[] = [];
        for (let r = top; r < buf.length; r++) {
          const line = buf.getLine(r);
          if (!line || (r > top && !line.isWrapped)) break;
          rows.push(line.translateToString(false));
        }
        const cols = term.cols;
        const text = rows.join("");
        const links = findLinks(text.replace(/\s+$/, "")).map((l) => {
          const at = (o: number) => ({ x: (o % cols) + 1, y: top + Math.floor(o / cols) + 1 });
          return { text: l.url, range: { start: at(l.start), end: at(l.end - 1) }, activate: (_e: MouseEvent, url: string) => { openUrl(url).catch((e) => hub.fail(e)); } };
        }).filter((l) => l.range.end.y >= y && l.range.start.y <= y);
        done(links);
      },
    });

    // Copy and paste as in Windows Terminal: Ctrl+C copies when something is selected (else it
    // interrupts), Ctrl+Shift+C copies, Ctrl+V and Ctrl+Shift+V paste.
    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown" || !(e.ctrlKey || e.metaKey)) return true;
      const key = e.key.toLowerCase();
      if (key === "c" && (e.shiftKey || term.hasSelection())) {
        if (term.hasSelection()) quiet(navigator.clipboard.writeText(term.getSelection()), "write text");
        term.clearSelection();
        e.preventDefault();
        return false;
      }
      // Leave it to the browser, whose paste event xterm takes as typed.
      if (key === "v") return false;
      return true;
    });

    let ws: WebSocket | null = null;
    let stopped = false;
    let retry: ReturnType<typeof setTimeout> | undefined;
    const enc = new TextEncoder();
    const send = (data: string | Uint8Array) => {
      if (ws?.readyState === WebSocket.OPEN) ws.send(typeof data === "string" ? enc.encode(data) : data);
    };
    sendKeys = (data) => send(data);
    const size = () => {
      if (ws?.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ resize: [term.cols, term.rows] }));
    };

    const connect = () => {
      ws = new WebSocket(socketUrl(`/api/terminals/${encodeURIComponent(id)}/ws`));
      ws.binaryType = "arraybuffer";
      let first = true;
      let opened = false;
      ws.onopen = () => {
        opened = true;
        lost = false;
        size();
        if (input) {
          send(input);
          input = undefined;
          ontyped?.();
        }
      };
      ws.onmessage = (e) => {
        if (typeof e.data === "string") {
          const msg = JSON.parse(e.data);
          if (typeof msg.exit === "number") {
            exit = msg.exit;
            onexit?.(msg.exit);
          }
          return;
        }
        // Each connection starts with everything so far: draw it on a clean screen.
        if (first) {
          term.reset();
          first = false;
        }
        term.write(new Uint8Array(e.data));
      };
      ws.onclose = () => {
        if (stopped || exit !== null) return;
        // Refused outright: the core forgot it (it restarted, or it was closed elsewhere).
        if (!opened) {
          lost = true;
          return;
        }
        retry = setTimeout(connect, 500);
      };
    };
    connect();

    const typed = term.onData((d) => send(d));
    const binary = term.onBinary((d) => send(Uint8Array.from(d, (c) => c.charCodeAt(0))));
    const resized = term.onResize(size);
    const ro = new ResizeObserver(() => {
      if (box.offsetWidth && box.offsetHeight) fit.fit();
    });
    ro.observe(box);
    fit.fit();
    // The terminal font can arrive after the first fit; a different cell size needs a new fit.
    document.fonts?.ready.then(() => {
      if (!stopped && box.offsetWidth && box.offsetHeight) fit.fit();
    });
    term.focus();

    return () => {
      stopped = true;
      clearTimeout(retry);
      ro.disconnect();
      typed.dispose();
      binary.dispose();
      resized.dispose();
      ws?.close();
      term.dispose();
    };
  });
</script>

<div class="pane">
  <div class="screen"><div class="fit" bind:this={box}></div></div>
  {#if exit !== null}
    <p class="end">Ended{exit ? ` with code ${exit}` : ""}.</p>
  {:else if lost}
    <p class="end bad">This terminal is gone; Hub may have restarted. Open a new one.</p>
  {/if}
</div>

<style>
  .pane { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--bg-sidebar); border: 1px solid var(--line); border-radius: var(--r-md); overflow: hidden; }
  .screen { user-select: text; -webkit-user-select: text; flex: 1; min-height: 0; padding: 8px 4px 4px 10px; display: flex; flex-direction: column; }
  .fit { flex: 1; min-height: 0; min-width: 0; overflow: hidden; }
  .fit :global(.xterm) { height: 100%; }
  .screen :global(.xterm-viewport) { background: transparent !important; }
  .end { margin: 0; padding: 6px 12px; font-size: 12px; color: var(--muted); border-top: 1px solid var(--line); }
  .end.bad { color: var(--red); }
</style>
