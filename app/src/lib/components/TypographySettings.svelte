<!-- Settings > Appearance: the chat's look and the fonts. Interface font is everything that is Hub's own, Chat font is what the agent writes, Code font is code and the terminal; each is picked from the fonts Hub carries or any font installed on this computer, by name. See fonts.ts.
     Used by Settings.svelte. -->
<script lang="ts">
  // Settings > Appearance: the chat's look and the fonts. Interface font is everything that is Hub's
  // own, Chat font is what the agent writes, Code font is code and the terminal; each is picked from
  // the fonts Hub carries or any font installed on this computer, by name. See fonts.ts.
  import { onMount } from "svelte";
  import { prefs, setPref } from "../prefs.svelte";
  import {
    BUNDLED, CHAT_LINES, CHAT_SIZE, CHAT_SIZES, CHAT_WIDTHS, CODE_SIZE, CODE_SIZES, LOOKS,
    bundledNames, canListInstalled, fontStack, installedFonts, loadFont, type ChatWidth, type FontKind,
  } from "../fonts";
  import Select, { type Option } from "./Select.svelte";

  // Installed fonts are only listed once the person has allowed it: asking on opening this page made
  // the webview pop up a permission box. Until then a font can still be typed in by name.
  let local = $state<string[]>([]);
  const SHOW = "\u0000show-installed";
  const canList = canListInstalled();
  onMount(() => {
    installedFonts().then((l) => (local = l)); // asks nothing: empty unless already allowed
    preload();
  });
  const showInstalled = () => installedFonts(true).then((l) => (local = l));
  /** A pick from a font list: the "Show installed fonts" row is an action, anything else is a font. */
  const pick = (key: "uiFont" | "chatFont" | "codeFont") => (v: string) => (v === SHOW ? void showInstalled() : setPref(key, v));

  /** Every font in a list shows in its own face, so Hub's fonts load when this page opens (about 50 KB each). */
  const preload = () => bundledNames("sans", "serif", "mono").forEach((n) => loadFont(n));

  const quote = (n: string) => `"${n.replace(/["\\;{}]/g, "")}"`;
  const bundledFont = (n: string) => fontStack(n, BUNDLED[n].kind) ?? undefined;

  /** The options of a font picker: the default first, Hub's own fonts, those installed here, and the current pick if it is neither. */
  function fonts(kinds: FontKind[], first: Option<string>, skip: string, current: string): Option<string>[] {
    const out: Option<string>[] = [first];
    for (const n of bundledNames(...kinds)) {
      if (n === skip) continue;
      out.push({ value: n, label: n, group: "Hub's fonts", hint: BUNDLED[n].kind === "serif" ? "serif" : undefined, font: bundledFont(n) });
    }
    const known = new Set([...bundledNames("sans", "serif", "mono"), ...local]);
    if (current && !known.has(current)) out.push({ value: current, label: current, group: "Your own", font: quote(current) });
    for (const n of local) out.push({ value: n, label: n, group: "Installed on this computer", font: quote(n) });
    if (!local.length && canList) out.push({ value: SHOW, label: "Show installed fonts…", group: "Installed on this computer", hint: "asks once" });
    return out;
  }
  const own = (typed: string): Option<string> => ({ value: typed, label: `Use “${typed}”`, group: "Your own", font: quote(typed) });

  const uiFonts = $derived(fonts(["sans"], { value: "", label: "Geist", hint: "default" }, "Geist", prefs.uiFont));
  const chatFonts = $derived(fonts(["sans", "serif"], { value: "", label: "Same as interface" }, "", prefs.chatFont));
  const codeFonts = $derived(fonts(["mono"], { value: "", label: "Geist Mono", hint: "default" }, "Geist Mono", prefs.codeFont));

  const sizes = (list: number[], was: number): Option<number>[] => list.map((n) => ({ value: n, label: `${n} px`, hint: n === was ? "default" : undefined }));
  const lines = CHAT_LINES.map((n): Option<number> => ({ value: n, label: String(n), hint: n === 1.65 ? "default" : undefined }));

  const active = $derived(
    LOOKS.find((l) => l.look.chatFont === prefs.chatFont && l.look.chatSize === prefs.chatSize && l.look.chatLine === prefs.chatLine && l.look.chatWidth === prefs.chatWidth)?.id ?? null,
  );
  const apply = (id: string) => {
    const l = LOOKS.find((x) => x.id === id)?.look;
    if (!l) return;
    setPref("chatFont", l.chatFont);
    setPref("chatSize", l.chatSize);
    setPref("chatLine", l.chatLine);
    setPref("chatWidth", l.chatWidth);
  };
  const reset = () => {
    apply("hub");
    setPref("uiFont", "");
    setPref("codeFont", "");
    setPref("codeSize", CODE_SIZE);
  };
  const isDefault = $derived(active === "hub" && !prefs.uiFont && !prefs.codeFont && prefs.codeSize === CODE_SIZE);
  const WIDTHS: [ChatWidth, string][] = [["narrow", "Narrow"], ["medium", "Medium"], ["wide", "Wide"]];
</script>

<h2>Look</h2>
<section>
  <div class="line">
    <div class="what"><div class="name">Chat look</div><div class="hint">A preset sets the chat's font, size, line spacing and width in one go. Change any of them below afterwards.</div></div>
    <div class="control seg" role="group" aria-label="Chat look">
      {#each LOOKS as l (l.id)}
        <button class:active={active === l.id} data-tip={l.hint} onclick={() => apply(l.id)}>{l.label}</button>
      {/each}
      <button class:active={active === null} disabled={active !== null} data-tip="Your own mix of the settings below">Custom</button>
    </div>
  </div>
</section>

<h2 class="with-action">Typography{#if !isDefault}<button class="link" onclick={reset}>Reset to Hub</button>{/if}</h2>
<section>
  <div class="preview" aria-hidden="true">
    <div class="mine"><span>Bara se till att det är fixat.</span></div>
    <div class="reply">
      <p><b>Cause:</b> The errors were not in code. This worktree’s <code>node_modules</code> was a link to an out-of-date install, so <code>katex</code> and <code>mermaid</code> were missing.</p>
      <pre><code><span class="del">- return user.name.toUpperCase();</span>
<span class="add">+ return user.name + " &lt;" + user.email + "&gt;";</span></code></pre>
    </div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Interface font</div><div class="hint">Everything that is Hub's own: lists, menus, buttons, your messages and the settings.</div></div>
    <div class="control"><Select options={uiFonts} value={prefs.uiFont} onchange={pick("uiFont")} searchable custom={own} label="Interface font" /></div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Chat font</div><div class="hint">What the agent writes. Same as interface keeps one typeface.</div></div>
    <div class="control">
      <Select options={chatFonts} value={prefs.chatFont} onchange={pick("chatFont")} searchable custom={own} label="Chat font" />
      <Select options={sizes(CHAT_SIZES, CHAT_SIZE)} value={prefs.chatSize} onchange={(v) => setPref("chatSize", v)} label="Chat text size" />
    </div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Code font</div><div class="hint">Code blocks, diffs, command chips and the terminal.</div></div>
    <div class="control">
      <Select options={codeFonts} value={prefs.codeFont} onchange={pick("codeFont")} searchable custom={own} mono label="Code font" />
      <Select options={sizes(CODE_SIZES, CODE_SIZE)} value={prefs.codeSize} onchange={(v) => setPref("codeSize", v)} label="Code text size" />
    </div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Line spacing</div><div class="hint">The height of a line in the agent's replies, as a multiple of the text size.</div></div>
    <div class="control"><Select options={lines} value={prefs.chatLine} onchange={(v) => setPref("chatLine", v)} label="Line spacing" /></div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Chat width</div><div class="hint">How wide replies and the message box grow on a large screen ({Object.values(CHAT_WIDTHS).join(" / ")} px).</div></div>
    <div class="control seg" role="group" aria-label="Chat width">
      {#each WIDTHS as [id, label] (id)}
        <button class:active={prefs.chatWidth === id} onclick={() => setPref("chatWidth", id)}>{label}</button>
      {/each}
    </div>
  </div>
</section>

<style>
  h2 { color: var(--text); font-size: 15px; font-weight: 600; margin: 34px 0 4px; }
  h2.with-action { display: flex; justify-content: space-between; align-items: center; }
  h2 .link { color: var(--muted); font-size: 12px; font-weight: 400; }
  h2 .link:hover { color: var(--accent); }
  section { margin-bottom: 4px; }
  .line { display: flex; flex-wrap: wrap; align-items: center; gap: 12px 32px; padding: 16px 0; }
  .line + .line { border-top: 1px solid var(--line); }
  .what { flex: 1; flex-basis: 260px; min-width: 0; }
  .name { color: var(--text); font-size: 14px; }
  .hint { color: var(--muted); font-size: 13px; margin-top: 3px; }
  .control { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; flex: none; max-width: 100%; }
  .seg { background: var(--bg); border-radius: var(--r-md); padding: 2px; gap: 2px; }
  .seg button { height: 24px; padding: 0 10px; border-radius: var(--r-sm); color: var(--text-3); font-size: 13px; }
  .seg button:hover:not(:disabled) { color: var(--text); }
  .seg button.active { background: var(--surface-hover); color: var(--text); }
  .seg button:disabled:not(.active) { opacity: 0.5; }

  /* A sample of the chat, in the same fonts and sizes the chat takes, so a pick shows at once. */
  .preview { margin: 12px 0 4px; padding: 16px 22px 14px; background: var(--bg-sidebar); border: 1px solid var(--line); border-radius: var(--r-lg); }
  .mine { display: flex; justify-content: flex-end; margin-bottom: 12px; }
  .mine span { background: var(--surface-hover); color: var(--text); padding: 8px 14px; border-radius: var(--r-lg); font-size: calc(14.5px * var(--chat-scale)); }
  .reply { font-family: var(--chat-font); font-size: calc(14.5px * var(--chat-scale)); line-height: var(--chat-lh); color: var(--text-2); }
  .reply p { margin: 0; }
  .reply b { color: var(--text); }
  .reply code { background: var(--surface-2); padding: 1px 4px; border-radius: var(--r-xs); color: var(--text-3); }
  .reply pre { margin: 10px 0 0; background: var(--bg); border: 1px solid var(--line); border-radius: var(--r-md); padding: 8px 12px; overflow-x: auto; line-height: 1.5; }
  .reply pre code { background: none; padding: 0; color: var(--text-2); }
  .del { color: var(--red); }
  .add { color: var(--green); }
</style>
