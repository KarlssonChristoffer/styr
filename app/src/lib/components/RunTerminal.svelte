<!-- A terminal under the reply that has the command: opened on the thread's machine, in its folder, with the command typed and not run. The person reads it and presses Enter. As administrator opens a root shell (`sudo -i`, which asks for its password in the terminal, so the command is typed after, with a button), or on this computer when it is Windows a window outside Hub.
     Used by Markdown.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { onMount, untrack } from "svelte";
  import { api } from "../api";
  import type { TerminalSession } from "../bindings/TerminalSession";
  import { hub } from "../state.svelte";
  import { bar } from "../statusbar.svelte";
  import { cdLine, typedCommand } from "../runInTerminal";
  import Icon from "./Icon.svelte";
  import TerminalPane from "./TerminalPane.svelte";

  // A terminal under the reply that has the command: opened on the thread's machine, in its folder,
  // with the command typed and not run. The person reads it and presses Enter. As administrator
  // opens a root shell (`sudo -i`, which asks for its password in the terminal, so the command is
  // typed after, with a button), or on this computer when it is Windows a window outside Hub.
  let { command, machine, cwd, admin = false, canAdmin, outside = false, elevated = false, onclose }: {
    command: string;
    machine: string;
    cwd: string | null;
    admin?: boolean;
    /** Whether the administrator switch is offered for this machine. */
    canAdmin: boolean;
    /** Administrator on this computer when it is Windows: a window outside Hub, the command copied. */
    outside?: boolean;
    /** Administrator on this computer when it is Windows and Hub is new enough: an elevated terminal inside Hub, typed like any other. */
    elevated?: boolean;
    onclose: () => void;
  } = $props();

  let root = $state(untrack(() => admin));
  let session = $state<TerminalSession | null>(null);
  let failed = $state<string | null>(null);
  let outsideNote = $state<string | null>(null);
  let pane = $state<TerminalPane>();
  let typed = $state(false);
  /** Which open this is, so a slow answer from the one before is closed instead of shown. */
  let gen = 0;

  async function open() {
    const mine = ++gen;
    const old = session;
    session = null;
    failed = null;
    typed = false;
    if (old) void quiet(api.closeTerminal(old.id), "close terminal");
    if (root && outside) {
      try {
        await api.openOnMachine(machine, { what: "shell", engine: null, admin: true });
        await quiet(navigator.clipboard.writeText(command), "write text");
        outsideNote = "Opened as administrator in a window outside Hub. The command is copied; paste it with Ctrl V.";
      } catch (e) {
        failed = String(e);
      }
      return;
    }
    outsideNote = null;
    try {
      const t = await api.machineTerminal(machine, { what: root && !elevated ? "root" : "shell", engine: null, thread: null, cols: 100, rows: 8, ...(root && elevated ? { admin: true } : {}) });
      if (mine !== gen) return void quiet(api.closeTerminal(t.id), "close terminal");
      session = t;
    } catch (e) {
      if (mine === gen) failed = String(e);
    }
  }
  onMount(() => {
    void open();
    return () => {
      // The card going away (the thread was left) keeps the terminal, in the panel below; only
      // its × ends it.
      if (session && !closed && !bar.mine.includes(session.id)) adopt(session);
    };
  });

  /** Typed when the terminal connects. A root shell is asked for a password first, so it waits for the button. */
  const first = $derived(root && !elevated ? undefined : (cwd ? cdLine(cwd) : "") + typedCommand(command));
  function typeNow() {
    pane?.type((cwd ? cdLine(cwd) : "") + typedCommand(command));
    typed = true;
  }
  function flip() {
    root = !root;
    void open();
  }
  let closed = false;
  function adopt(t: TerminalSession) {
    bar.mine = [...bar.mine, t.id];
    bar.terminals = [...bar.terminals, t];
  }
  function popOut() {
    if (!session) return;
    adopt(session);
    bar.terminal = session.id;
    bar.panel = "terminal";
    onclose();
  }
  function close() {
    closed = true;
    if (session) void quiet(api.closeTerminal(session.id), "close terminal");
    onclose();
  }
</script>

<div class="run" class:root>
  <div class="hd">
    <Icon name="terminal" size={13} />
    <span class="chip">{machine}</span>
    {#if cwd}<span class="chip"><Icon name="folder" size={11} />{cwd}</span>{/if}
    {#if root}<span class="chip adm">Administrator</span>{/if}
    <span class="grow"></span>
    {#if canAdmin}
      <button class="adm-t" class:on={root} aria-pressed={root} data-tip={root ? "Back to a normal terminal" : "Open it as administrator instead"} onclick={flip}>
        Administrator <i class="sw"></i>
      </button>
    {/if}
    {#if session}<button class="ib" data-tip="Move to the panel below" aria-label="Move to the panel below" onclick={popOut}><Icon name="popout" size={13} /></button>{/if}
    <button class="ib" data-tip="Close" aria-label="Close" onclick={close}><Icon name="x" size={13} /></button>
  </div>
  {#if outsideNote}
    <p class="note">{outsideNote}</p>
  {:else if failed}
    <p class="note bad">{failed}</p>
  {:else if session}
    <div class="term">
      {#key session.id}<TerminalPane bind:this={pane} id={session.id} input={first} />{/key}
    </div>
    <div class="ft">
      {#if root && !elevated && !typed}
        <button class="type" onclick={typeNow}>Type the command</button><span>after the password</span>
      {:else}
        <span><b>Enter</b>runs it</span><span>Typed, not run</span>
      {/if}
    </div>
  {:else}
    <p class="note">Opening…</p>
  {/if}
</div>

<style>
  .run { margin: -1px 0 12px; border: 1px solid var(--line); border-radius: var(--r-md); background: var(--bg-sidebar); overflow: hidden; font-family: var(--font, inherit); }
  .hd { display: flex; align-items: center; gap: 8px; padding: 4px 8px 4px 10px; border-bottom: 1px solid var(--line); color: var(--muted); font-size: 12px; min-width: 0; }
  .chip { display: inline-flex; align-items: center; gap: 4px; padding: 1px 8px; border-radius: var(--r-sm); background: var(--surface-hover); color: var(--text-2); max-width: 22em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chip.adm { background: var(--amber-bg); color: var(--amber); }
  .grow { flex: 1; }
  .adm-t { display: inline-flex; align-items: center; gap: 6px; background: none; border: 0; padding: 0 4px; font: inherit; color: var(--muted); cursor: pointer; }
  .adm-t:hover { color: var(--text-2); }
  .sw { display: inline-block; width: 18px; height: 10px; border-radius: var(--r-sm); background: var(--line-strong); position: relative; flex: none; }
  .sw::after { content: ""; position: absolute; top: 2px; left: 2px; width: 6px; height: 6px; border-radius: 50%; background: var(--muted); transition: left 0.12s var(--ease); }
  .on .sw { background: var(--amber-bg); }
  .on .sw::after { left: 10px; background: var(--amber); }
  .ib { display: grid; place-items: center; width: 22px; height: 22px; border-radius: var(--r-sm); background: none; border: 0; color: var(--muted); cursor: pointer; }
  .ib:hover { background: var(--surface-hover); color: var(--text); }
  .term { height: 150px; }
  .term :global(.pane) { border: 0; border-radius: 0; }
  .ft { display: flex; align-items: center; gap: 14px; padding: 4px 12px 8px; color: var(--muted); font-size: 12px; }
  .ft b { font-weight: 400; border: 1px solid var(--line-strong); border-radius: var(--r-xs); padding: 0 4px; margin-right: 4px; color: var(--text-3); font-size: 11px; }
  .type { background: var(--accent-bg); border: 0; border-radius: var(--r-sm); padding: 2px 8px; font: inherit; color: var(--text); cursor: pointer; }
  .note { margin: 0; padding: 10px 12px; font-size: 13px; color: var(--muted); }
  .note.bad { color: var(--red); }
</style>
