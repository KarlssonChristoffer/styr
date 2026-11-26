<!-- A new project from just a name. Two columns: where it lives (the machines as rows, then the folder, browsed in place on that machine) and what it is (name, a starter, the extras as switches). The folder goes in the default folder (D:\Dev), becomes a git repository with a plan, optionally a GitHub repository, and a setup thread picks the stack and scaffolds it. The starter card opens the list in stacks.ts or "Not sure what to use?", which suggests stacks for what you want to build. An existing folder can be added as it is.
     Used by App.svelte. -->
<script lang="ts">
  import { autofocus } from "../autofocus";
  import { quiet, quietly } from "../quiet";
  import { pop } from "../popover";
  import { modal } from "../pop";
  import Quiet from "./Quiet.svelte";
  import { untrack } from "svelte";
  import { api, type NewProjectMore } from "../api";
  import type { FolderListing } from "../bindings/FolderListing";
  import type { MachineAbout } from "../bindings/MachineAbout";
  import type { ProjectDefaults } from "../bindings/ProjectDefaults";
  import type { ToolStatus } from "../bindings/ToolStatus";
  import { hubdHas } from "../compat";
  import { bytes, isChatThread } from "../format";
  import { inTauri, pickFolder } from "../native";
  import { GLYPHS, machineGlyph } from "../refs";
  import { GRID, KINDS, STACKS, stackHint } from "../stacks";
  import { hub, recentFolders } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import Select from "./Select.svelte";
  import StackLogo from "./StackLogo.svelte";
  import Switch from "./Switch.svelte";

  // A new project from just a name. Two columns: where it lives (the machines as rows, then the
  // folder, browsed in place on that machine) and what it is (name, a starter, the extras as
  // switches). The folder goes in the default folder (D:\Dev), becomes a git repository with a
  // plan, optionally a GitHub repository, and a setup thread picks the stack and scaffolds it. The
  // starter card opens the list in stacks.ts or "Not sure what to use?", which suggests stacks for
  // what you want to build. An existing folder can be added as it is.
  let { onclose: close }: { onclose: () => void } = $props();
  // A quick thread being made a project: its title is the name and `about` says where its files are.
  const from = hub.newProjectFrom;
  const onclose = () => {
    hub.newProjectFrom = null;
    close();
  };

  let mode = $state<"new" | "existing">("new");
  let name = $state(from?.name ?? "");
  let about = $state(from?.about ?? "");
  let defaults = $state<ProjectDefaults | null>(null);
  let parent = $state("");
  let makeDefault = $state(false);
  let github = $state(true);
  let setup = $state(true);

  // The machine it is made on ("" is this one), and what its hubd can do.
  let host = $state(from?.host && from.host !== hub.localHost ? from.host : "");
  let templates = $state(false);
  // From that machine's hubd: the templates it writes itself, and which tools it has.
  let provided = $state<string[]>([]);
  let tools = $state<ToolStatus[] | null>(null);
  let tpl = $state("blank");
  let tab = $state<"templates" | "help">("templates");
  let kind = $state(KINDS[0].kind);
  let publicRepo = $state(false);
  let ci = $state(false);
  let trees = $state(false);

  // What the right column shows: the form, or the folder browser or the starter list in its place.
  let stage = $state<"form" | "folder" | "starter">("form");
  let abouts = $state<Record<string, MachineAbout | null>>({});
  type Machine = { value: string; label: string; local: boolean; online: boolean; name: string };
  const machines = $derived<Machine[]>([
    { value: "", label: "This computer", local: true, online: true, name: hub.localHost },
    ...hub.hosts.filter((h) => !h.local).map((h) => ({ value: h.name, label: h.name, local: false, online: h.state === "online", name: h.name })),
  ]);
  const machine = $derived(machines.find((m) => m.value === host) ?? machines[0]);
  const sub = (m: Machine) => {
    if (!m.online) return "offline";
    const a = abouts[m.value];
    return [a?.os, a?.disk ? `${bytes(a.disk.free)} free` : ""].filter(Boolean).join(" · ") || (m.local ? "" : "online");
  };
  // Each online machine says what it is and how much disk it has, once.
  $effect(() => {
    for (const m of machines) {
      if (!m.online) continue;
      const key = m.value;
      if (untrack(() => key in abouts)) continue;
      untrack(() => (abouts[key] = null));
      quiet(api.hostAbout(key || null).then((a) => (abouts[key] = a)), "host about");
    }
  });
  const stack = $derived(STACKS[tpl]);
  /** The setup thread has to start this stack: the core cannot write it, or it has no files for it. */
  /** The core writes this starter itself (a hubd with feature `project.templates`). */
  const core = $derived(tpl !== "blank" && templates && provided.includes(tpl));
  const scaffold = $derived(tpl !== "blank" && !core);
  const toolOf = (n: string) => tools?.find((t) => t.tool === n);
  /** The tools the stack needs that the machine reported as missing. */
  const lacking = $derived(stack.needs.filter((n) => toolOf(n) && !toolOf(n)?.found));
  const where = $derived(host ? host : "this computer");
  const described = $derived(about.trim().length > 0);
  const advice = $derived(KINDS.find((k) => k.kind === kind)?.advice ?? []);
  const needsNext = $derived(templates ? "" : host ? "needs a newer Hub on that machine" : "needs the next version of Hub");

  let folder = $state("");
  let missing = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  // Where projects go and whether GitHub works are the chosen machine's own.
  $effect(() => {
    const h = host;
    untrack(() => {
      parent = "";
      stage = "form";
    });
    api
      .projectDefaults(h || null)
      .then((d) => {
        if (h !== host) return;
        defaults = d;
        parent = d.root;
        github = d.github;
      })
      .catch(quietly("new project defaults"));
  });
  $effect(() => {
    const h = host;
    (h ? quiet(api.featuresOf(h).then((f) => f.has("project.templates")), "features of", false) : hubdHas("project.templates")).then((has) => { // compat-until: 2026-12-03
      if (h !== host) return;
      templates = has;
      provided = [];
      tools = null;
      if (!has) return;
      api
        .projectTemplates(h || null)
        .then((t) => {
          if (h !== host) return;
          provided = t.templates;
          tools = t.tools;
        })
        .catch(quietly("project templates"));
    });
  });

  const folders = $derived([...new Set([...recentFolders(), ...hub.threads.filter((t) => !isChatThread(t)).map((t) => t.cwd)])]);
  // Recent folders as Hub's own menu; the WebView's datalist looks foreign.
  let suggestOpen = $state(false);
  const suggestions = $derived.by(() => {
    const q = folder.trim().toLowerCase();
    const added = new Set(hub.projects.map((p) => p.path));
    return folders.filter((f) => !added.has(f) && f.toLowerCase() !== q && f.toLowerCase().includes(q)).slice(0, 6);
  });

  const sep = $derived(parent.includes("\\") || /^[A-Za-z]:/.test(parent) ? "\\" : "/");
  /** Folder name for a project name: spaces become dashes, characters Windows forbids go. */
  const folderFor = (n: string) => n.trim().replace(/[<>:"/\\|?*]/g, "").replace(/\s+/g, "-");
  const target = $derived(
    mode === "existing"
      ? folder.trim()
      : parent.trim() && folderFor(name)
        ? parent.trim().replace(/[\\/]+$/, "") + sep + folderFor(name)
        : "",
  );

  async function browse(which: "folder" | "parent") {
    const picked = await pickFolder(which === "folder" ? folder : parent);
    if (!picked) return;
    if (which === "folder") {
      folder = picked;
      missing = false;
    } else parent = picked;
  }

  function pick(id: string) {
    tpl = id;
    stage = "form";
  }

  // The folder browser: the folders of the chosen machine, one level at a time.
  let listing = $state<FolderListing | null>(null);
  let listError = $state<string | null>(null);
  let listBusy = $state(false);
  let pathInput = $state("");
  let asked = 0;
  async function list(path: string) {
    const mine = ++asked;
    listBusy = true;
    try {
      const l = await api.folders(path, host || null);
      if (mine !== asked) return;
      listing = l;
      pathInput = l.path;
      listError = null;
    } catch (err) {
      if (mine === asked) listError = err instanceof Error ? err.message : String(err);
    } finally {
      if (mine === asked) listBusy = false;
    }
  }
  function browseFolder() {
    stage = "folder";
    listing = null;
    list(parent.trim() || "~");
  }
  function useFolder() {
    if (listing) parent = listing.path;
    stage = "form";
  }

  async function submit(e?: SubmitEvent, createMissing = false) {
    e?.preventDefault();
    if (!target || busy) return;
    busy = true;
    error = null;
    try {
      if (mode === "new" && !host && makeDefault && parent.trim() && parent.trim() !== defaults?.root) {
        await api.setProjectRoot(parent.trim());
      }
      let ask: Parameters<typeof hub.addProject>[0];
      if (mode === "new") {
        // A stack the core does not write itself goes to the setup thread by name.
        const text = [about.trim(), scaffold ? stackHint(tpl) : ""].filter(Boolean).join("\n\n");
        const more: NewProjectMore = {};
        if (tpl !== "blank") more.template = tpl;
        if (templates && github && publicRepo) more.public = true;
        if (templates && github && ci) more.ci = true;
        ask = { path: target, create: true, name: name.trim(), github, setup: setup || scaffold || !!from, about: text || undefined, ...more };
      } else ask = { path: target, create: createMissing };
      const made = await hub.addProject(ask, mode === "new" && host ? host : null);
      if (mode === "new" && trees && !host) await quiet(api.setProjectTrees(made.path, true), "set project trees");
      onclose();
      if (made.notes.length) hub.fail(new Error(made.notes.join(" ")));
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      // A folder typed by hand that is not there yet: offer to create it instead of failing.
      if (mode === "existing" && /does not exist/.test(message)) missing = true;
      else error = message;
    } finally {
      busy = false;
    }
  }

  function keydown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (stage !== "form") stage = "form";
    else onclose();
  }

</script>

<svelte:window onkeydown={keydown} />

{#snippet glyph(path: string, size = 22)}
  <svg class="g" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true"><path d={path} /></svg>
{/snippet}

<div role="presentation" class="backdrop" in:modal|global out:modal|global={{ out: true }} onclick={(e) => e.target === e.currentTarget && onclose()}>
  <form class="dialog" class:two={mode === "new"} onsubmit={submit} aria-label="New project">
    <div class="top">
      <h2>New project</h2>
      <div class="seg" role="tablist">
        <button type="button" role="tab" aria-selected={mode === "new"} class:on={mode === "new"} onclick={() => (mode = "new")}>New</button>
        <button type="button" role="tab" aria-selected={mode === "existing"} class:on={mode === "existing"} onclick={() => (mode = "existing")}>Existing folder</button>
      </div>
    </div>

    {#if mode === "new"}
      <div class="cols">
        <div class="rail">
          <div class="lbl">Where it lives</div>
          <div class="machines" role="radiogroup" aria-label="Machine">
            {#each machines as m (m.value)}
              <button
                type="button"
                class="mrow"
                class:on={m.value === machine.value}
                class:off={!m.online}
                role="radio"
                aria-checked={m.value === machine.value}
                disabled={!m.online}
                onclick={() => (host = m.value)}
              >
                {@render glyph(GLYPHS[machineGlyph(m.name, m.local)])}
                <span class="who">
                  <span class="nm">{m.label}{#if m.online}<i class="up"></i>{/if}</span>
                  <span class="sub">{sub(m)}</span>
                </span>
                {#if m.value === machine.value}<span class="tick"><Icon name="check" size={14} /></span>{/if}
              </button>
            {/each}
          </div>
          <div class="lbl folder-lbl">Folder</div>
          <div class="folder">
            <Icon name="folder" size={14} />
            <span class="path mono" title={target}>{parent || "…"}{#if target}{sep}<b>{folderFor(name)}</b>{/if}</span>
            <button type="button" class="link" onclick={browseFolder} disabled={!parent}>Browse</button>
          </div>
        </div>

        <div class="main">
          {#if stage === "folder"}
            <div class="stage-head">
              <span class="lbl">Folder on {machine.label.toLowerCase() === "this computer" ? "this computer" : machine.label}</span>
              <span class="grow"></span>
              <button type="button" class="link" onclick={() => (stage = "form")}>Cancel</button>
            </div>
            <div class="pathbar">
              <button type="button" class="icon" aria-label="Up one folder" disabled={!listing?.parent} onclick={() => listing?.parent && list(listing.parent)}>
                <span class="rot"><Icon name="chevron" size={13} /></span>
              </button>
              <input
                class="mono"
                bind:value={pathInput}
                spellcheck="false"
                autocomplete="off"
                aria-label="Folder"
                onkeydown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    list(pathInput.trim() || "~");
                  }
                }}
              />
            </div>
            {#if listing && listing.roots.length > 1}
              <div class="roots">
                {#each listing.roots as r (r)}
                  <button type="button" class="root" class:on={listing.path.toLowerCase().startsWith(r.toLowerCase())} onclick={() => list(r)}>{r}</button>
                {/each}
              </div>
            {/if}
            <div class="flist" class:busy={listBusy}>
              {#if listError}
                <p class="error">{listError}</p>
              {:else if listing}
                {#each listing.entries as e (e.path)}
                  <button type="button" class="frow" onclick={() => list(e.path)}>
                    <Icon name="folder" size={14} />
                    <span class="fname">{e.name}</span>
                    {#if e.repo}<span class="git">{@render glyph(GLYPHS.branch, 11)} git</span>{/if}
                  </button>
                {:else}
                  <p class="m empty">No folders in here.</p>
                {/each}
                {#if listing.more}<p class="m empty">There are more folders than shown. Type the path above.</p>{/if}
              {:else}
                <Quiet kind="loading" />
              {/if}
            </div>
            <div class="stage-foot">
              <span class="m grow">
                {#if listing}The project goes in <span class="mono t3">{listing.path.replace(/[\\/]+$/, "")}{listing.sep}{folderFor(name) || "…"}</span>{/if}
              </span>
              {#if inTauri && !host}<button type="button" class="link" onclick={() => browse("parent")}>System dialog</button>{/if}
              <button type="button" class="btn primary" onclick={useFolder} disabled={!listing}>Use this folder</button>
            </div>
            {#if listing && !host && listing.path !== defaults?.root}
              <div class="set"><span class="lbl2">Use this folder for new projects from now on</span><Switch on={makeDefault} label="Use this folder for new projects" onchange={(v) => (makeDefault = v)} /></div>
            {/if}
          {:else if stage === "starter"}
            <div class="stage-head">
              <div class="tabs2">
                <button type="button" class:on={tab === "templates"} onclick={() => (tab = "templates")}>Templates</button>
                <button type="button" class:on={tab === "help"} onclick={() => (tab = "help")}>Not sure what to use?</button>
              </div>
              <span class="grow"></span>
              <button type="button" class="link" onclick={() => (stage = "form")}>Back</button>
            </div>
            {#if tab === "templates"}
              <div class="chips">
                {#each GRID as id (id)}
                  <button type="button" class="chip" class:on={tpl === id} onclick={() => pick(id)}>
                    <StackLogo id={STACKS[id].logo} size={16} />
                    <span class="cn">{STACKS[id].name}<span class="cs">{STACKS[id].sub}</span></span>
                  </button>
                {/each}
              </div>
            {:else}
              <div class="help">
                <div class="kinds">
                  <div class="m small">I want to build a</div>
                  {#each KINDS as k (k.kind)}
                    <button type="button" class="kind" class:on={kind === k.kind} onclick={() => (kind = k.kind)}>{k.kind}</button>
                  {/each}
                </div>
                <div class="stacks">
                  {#each advice as a (a.stack)}
                    {@const s = STACKS[a.stack]}
                    <div class="stack">
                      <StackLogo id={s.logo} size={18} />
                      <div class="t">{s.name}<span class="m">{s.sub}</span>{#if a.recommended}<span class="tag">Recommended</span>{/if}</div>
                      <button type="button" class="link use" onclick={() => pick(a.stack)}>Use <Icon name="chevron" size={11} /></button>
                      <div class="g2"><b>Good at</b> {a.good}<br /><i>Watch out</i> {a.watch}</div>
                    </div>
                  {/each}
                </div>
              </div>
            {/if}
          {:else}
            <input class="name" bind:value={name} placeholder="Name, e.g. Untitled game" spellcheck="false" autocomplete="off" use:autofocus />
            <textarea class="about" bind:value={about} rows="2" placeholder="What is it? (optional)"></textarea>

            <div class="lbl starter-lbl">Starter</div>
            <button type="button" class="starter" onclick={() => (stage = "starter")}>
              <span class="logo"><StackLogo id={stack.logo} size={22} /></span>
              <span class="sbody">
                <span class="sname">{stack.name}{#if tpl !== "blank"}<span class="m">{stack.sub}</span>{/if}</span>
                <span class="m sline">{stack.blurb}</span>
                {#if stack.needs.length || stack.run}
                  <span class="sline needs">
                    {#if stack.needs.length}
                      <span class="m">Needs</span>
                      {#each stack.needs as n (n)}
                        {@const t = toolOf(n)}
                        <span class="need"><code class:lack={t && !t.found} title={t ? (t.found ? `found on ${where}${t.version ? `, version ${t.version}` : ""}` : `not found on ${where}`) : undefined}>{n}</code>{#if t?.found}<span class="ok"> ✓</span>{/if}</span>
                      {/each}
                    {/if}
                    {#if stack.run}<span class="m">Run</span> <code>{stack.run}</code>{/if}
                  </span>
                {/if}
                {#if lacking.length}
                  <span class="sline warn">Not found on {where}: {lacking.join(", ")}. <span class="m">{scaffold ? "The setup thread installs what it can." : "Install it before you run the project; the files are written either way."}</span></span>
                {/if}
                {#if scaffold}<span class="sline m">The setup thread starts it with the stack's own generator.</span>{/if}
              </span>
              <span class="link change">{tpl === "blank" ? "Choose" : "Change"} <Icon name="chevron" size={11} /></span>
            </button>

            <div class="lbl also-lbl">Also</div>
            <div class="switches">
              <div class="set" class:off={!defaults?.github}>
                <span class="lbl2">GitHub repository <span class="m">{defaults?.github ? `${publicRepo && templates ? "public" : "private"} · made with gh, first commit pushed` : "needs the GitHub CLI: install gh and run gh auth login"}</span></span>
                {#if github && templates}
                  <Select
                    size="small"
                    label="Repository visibility"
                    value={publicRepo ? "public" : "private"}
                    onchange={(v) => (publicRepo = v === "public")}
                    options={[{ value: "private", label: "Private" }, { value: "public", label: "Public" }]}
                  />
                {/if}
                <Switch on={github} label="GitHub repository" disabled={!defaults?.github} onchange={(v) => (github = v)} />
              </div>
              <div class="set" class:off={scaffold}>
                <span class="lbl2">Setup thread <span class="m">{scaffold ? "starts the stack you picked and writes the plan and docs" : core ? "only starts when you describe the project above" : "picks the stack, scaffolds it, writes the plan and docs, and asks if something is unclear"}</span></span>
                <Switch on={setup || scaffold} label="Setup thread" disabled={scaffold} onchange={(v) => (setup = v)} />
              </div>
              <div class="set" class:off={!templates || !github || !core || !stack.test}>
                <span class="lbl2">Build and test on GitHub <span class="m">{!templates ? needsNext : !github ? "turn on the GitHub repository first" : !core ? "needs a starter Hub writes itself" : !stack.test ? "this starter has no tests to run yet" : "a workflow that builds and runs the tests on every push"}</span></span>
                <Switch on={ci && !!github && core && !!stack.test} label="Build and test on GitHub" disabled={!templates || !github || !core || !stack.test} onchange={(v) => (ci = v)} />
              </div>
              <div class="set" class:off={!!host}>
                <span class="lbl2">Own worktree per thread <span class="m">{host ? "set on the project page of that machine" : "each thread works on a branch of its own"}</span></span>
                <Switch on={trees} label="Own worktree per thread" disabled={!!host} onchange={(v) => (trees = v)} />
              </div>
            </div>
          {/if}
        </div>
      </div>
    {:else}
      <div class="field">
        <div class="input">
          <Icon name="folder" size={14} />
          <input
            bind:value={folder}
            placeholder="F:\Dev\Atlas"
            spellcheck="false"
            autocomplete="off"
            use:autofocus
            oninput={() => (missing = false)}
            onfocus={() => (suggestOpen = true)}
            onblur={() => setTimeout(() => (suggestOpen = false), 120)}
          />
          {#if inTauri}<button type="button" class="browse" onclick={() => browse("folder")}>Browse…</button>{/if}
        </div>
        {#if suggestOpen && suggestions.length}
          <div class="suggest" transition:pop|global>
            <div class="suggest-label">Recent folders</div>
            {#each suggestions as f (f)}
              <button type="button" class="suggest-row mono" onmousedown={(e) => e.preventDefault()} onclick={() => { folder = f; suggestOpen = false; missing = false; }}>{f}</button>
            {/each}
          </div>
        {/if}
      </div>
      {#if missing}
        <div class="missing">
          <span><code>{target}</code> does not exist yet.</span>
          <button type="button" class="btn primary" onclick={() => submit(undefined, true)} disabled={busy}>Create it</button>
        </div>
      {/if}
    {/if}

    {#if error}<p class="error">{error}</p>{/if}
    <div class="actions" class:split={mode === "new"}>
      {#if mode === "new"}
        <span class="summary">
          Creates <b>{name.trim() || "…"}</b> on <b>{machine.label}</b>{#if parent}{" in "}<span class="mono">{parent}</span>{/if}{tpl === "blank" ? "" : ` from the ${stack.name} starter`}
        </span>
      {/if}
      <button type="button" class="btn" onclick={onclose}>Cancel</button>
      <button type="submit" class="btn primary" disabled={!target || busy}>
        {busy ? (mode === "new" ? "Creating…" : "Adding…") : mode === "new" ? "Create project" : "Add project"}
      </button>
    </div>
  </form>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.35); z-index: 50; display: flex; justify-content: center; align-items: flex-start; padding-top: 6vh; }
  .dialog { width: min(560px, calc(100vw - 32px)); background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.5); padding: 18px 20px 16px; max-height: calc(100vh - 6vh - 20px); overflow-y: auto; }
  .dialog.two { width: min(800px, calc(100vw - 32px)); }
  .top { display: flex; align-items: center; gap: 12px; margin-bottom: 14px; }
  h2 { color: var(--text); font-size: 15px; font-weight: 500; margin: 0; flex: 1; }
  .seg { display: inline-flex; gap: 2px; background: var(--bg); border: 1px solid var(--line); border-radius: var(--r-md); padding: 2px; }
  .seg button { color: var(--muted); padding: 4px 12px; border-radius: var(--r-md); font-size: 13px; }
  .seg button:hover { color: var(--text-2); }
  .seg button.on { background: var(--surface-hover); color: var(--text); }

  .cols { display: grid; grid-template-columns: 262px minmax(0, 1fr); gap: 18px; min-height: 440px; align-content: start; }
  @media (max-width: 720px) { .cols { grid-template-columns: 1fr; } }
  .lbl { font-size: 12px; color: var(--muted); letter-spacing: 0.02em; margin: 0 0 6px; }
  .rail .folder-lbl { margin-top: 12px; padding-top: 10px; border-top: 1px solid var(--line); }
  .machines { display: flex; flex-direction: column; gap: 2px; }
  .mrow { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: var(--r-md); border: 1px solid transparent; text-align: left; min-width: 0; width: 100%; }
  .mrow:hover:not(:disabled) { background: var(--surface-hover); }
  .mrow.on { border-color: var(--accent); background: var(--accent-bg); }
  .mrow.off { opacity: 0.55; cursor: default; }
  .g { flex: none; width: 22px; height: 22px; fill: none; stroke: var(--text-3); stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
  .who { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .nm { color: var(--text); font-size: 13px; display: flex; align-items: center; gap: 6px; min-width: 0; }
  .nm { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sub { color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tick { color: var(--green); display: grid; place-items: center; flex: none; }
  .folder { display: flex; align-items: center; gap: 8px; padding: 0 4px; color: var(--text-3); min-width: 0; }
  .folder .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; color: var(--text-3); }
  .folder .path b { font-weight: 400; color: var(--text); }
  .link { color: var(--accent); font-size: 13px; }
  .link:hover:not(:disabled) { text-decoration: underline; }
  .link:disabled { color: var(--muted); }
  .grow { flex: 1; }
  .mono { font-family: var(--mono, "Geist Mono Variable", monospace); }
  .t3 { color: var(--text-3); }
  .m { color: var(--muted); }

  .name { width: 100%; background: none; border: 0; outline: none; font-size: 20px; color: var(--text); padding: 2px 0 6px; }
  .name::placeholder, .about::placeholder { color: var(--muted); }
  .about { width: 100%; resize: none; background: var(--bg); border: 1px solid var(--line); border-radius: var(--r-md); outline: none; padding: 8px 10px; font-size: 13px; line-height: 1.5; color: var(--text-2); margin: 0; }
  .about:focus { border-color: var(--line-strong); }
  .starter-lbl, .also-lbl { margin-top: 14px; }
  .starter { display: flex; align-items: flex-start; gap: 12px; width: 100%; text-align: left; border: 1px solid var(--line-strong); border-radius: var(--r-lg); padding: 11px 12px; }
  .starter:hover { border-color: color-mix(in srgb, var(--line-strong) 60%, var(--text-3)); }
  .logo { flex: none; width: 22px; display: grid; place-items: center; margin-top: 1px; }
  .sbody { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
  .sname { color: var(--text); font-size: 13px; }
  .sname .m { margin-left: 8px; font-size: 12px; }
  .sline { font-size: 12px; line-height: 1.55; color: var(--text-3); overflow-wrap: anywhere; }
  .sline.needs { display: flex; flex-wrap: wrap; gap: 2px 8px; align-items: baseline; }
  .sline.warn { color: var(--amber); }
  .sline code { font-family: var(--mono, "Geist Mono Variable", monospace); font-size: 12px; color: var(--text-2); }
  .sline code.lack { color: var(--amber); }
  .ok { color: var(--green); }
  .need { display: inline-flex; align-items: baseline; gap: 4px; }
  .change { flex: none; white-space: nowrap; }
  .change :global(svg) { vertical-align: -1px; }

  .switches { display: flex; flex-direction: column; }
  .set { display: flex; align-items: center; gap: 12px; padding: 8px 0; font-size: 13px; color: var(--text-2); }
  .set .lbl2 { flex: 1; min-width: 0; }
  .set .lbl2 .m { font-size: 12px; margin-left: 4px; }
  .set.off .lbl2 { color: var(--muted); }

  /* The folder browser and the starter list take the right column. */
  .stage-head { display: flex; align-items: center; gap: 12px; margin-bottom: 8px; min-height: 22px; }
  .stage-head .lbl { margin: 0; }
  .pathbar { display: flex; align-items: center; gap: 6px; background: var(--bg); border: 1px solid var(--line); border-radius: var(--r-md); padding: 0 6px; height: 34px; }
  .pathbar:focus-within { border-color: var(--line-strong); }
  .pathbar input { flex: 1; min-width: 0; background: none; border: 0; outline: none; font-size: 13px; color: var(--text-2); }
  .pathbar .icon { color: var(--text-3); width: 22px; height: 22px; display: grid; place-items: center; border-radius: var(--r-sm); }
  .pathbar .icon:hover:not(:disabled) { background: var(--surface-hover); color: var(--text); }
  .pathbar .icon:disabled { opacity: 0.4; }
  .rot { display: inline-block; transform: rotate(-90deg); }
  .roots { display: flex; gap: 4px; margin-top: 6px; flex-wrap: wrap; }
  .root { font-size: 12px; color: var(--muted); padding: 2px 8px; border-radius: var(--r-sm); border: 1px solid var(--line); }
  .root:hover { color: var(--text-2); }
  .root.on { color: var(--text); border-color: var(--accent); background: var(--accent-bg); }
  .flist { border: 1px solid var(--line); border-radius: var(--r-lg); margin-top: 8px; padding: 4px; height: 252px; overflow-y: auto; }
  .flist.busy { opacity: 0.6; }
  .frow { display: flex; align-items: center; gap: 8px; width: 100%; text-align: left; padding: 6px 8px; border-radius: var(--r-md); color: var(--text-2); font-size: 13px; }
  .frow:hover { background: var(--surface-hover); color: var(--text); }
  .frow :global(svg) { color: var(--text-3); flex: none; }
  .fname { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .git { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: var(--muted); flex: none; }
  .git .g { width: 11px; height: 11px; stroke: var(--muted); }
  .empty { font-size: 13px; padding: 8px; margin: 0; }
  .stage-foot { display: flex; align-items: center; gap: 12px; margin-top: 10px; font-size: 12px; min-height: 30px; }
  .stage-foot .m { overflow-wrap: anywhere; }

  .field { display: block; margin-bottom: 10px; position: relative; }
  .input { display: flex; align-items: center; gap: 8px; background: var(--bg); border: 1px solid var(--line); border-radius: var(--r-md); padding: 0 8px; height: 34px; color: var(--muted); }
  .input:focus-within { border-color: var(--line-strong); }
  .input input { flex: 1; min-width: 0; background: none; border: 0; outline: none; font-size: 13px; }
  .browse { color: var(--text-3); font-size: 12px; padding: 2px 8px; border-radius: var(--r-sm); background: var(--surface-hover); }
  .browse:hover { color: var(--text); }
  .suggest { position: absolute; left: 0; right: 0; top: calc(100% + 4px); z-index: 2; background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 4px; box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45); }
  .suggest-label { color: var(--muted); font-size: 12px; padding: 3px 8px 4px; }
  .suggest-row { display: block; width: 100%; text-align: left; padding: 6px 8px; border-radius: var(--r-sm); color: var(--text-2); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .suggest-row:hover { background: var(--surface-hover); color: var(--text); }
  .missing { display: flex; align-items: center; gap: 12px; color: var(--text-3); font-size: 13px; margin: 4px 0 12px; overflow-wrap: anywhere; }
  .missing span { flex: 1; }
  .error { color: var(--red); font-size: 13px; margin: 10px 0 0; }
  .actions { display: flex; justify-content: flex-end; align-items: center; gap: 8px; margin-top: 16px; }
  .actions.split { border-top: 1px solid var(--line); padding-top: 12px; }
  .summary { flex: 1; min-width: 0; color: var(--muted); font-size: 13px; overflow-wrap: anywhere; }
  .summary b { font-weight: 400; color: var(--text-3); }
  .summary .mono { color: var(--text-3); }

  .tabs2 { display: flex; gap: 14px; font-size: 13px; }
  .tabs2 button { color: var(--muted); padding-bottom: 2px; border-bottom: 1px solid transparent; }
  .tabs2 button:hover { color: var(--text-2); }
  .tabs2 button.on { color: var(--text); border-bottom-color: var(--accent); }
  .chips { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 6px; }
  .chip { display: flex; align-items: center; gap: 10px; border: 1px solid var(--line); background: var(--bg); border-radius: var(--r-md); padding: 8px 10px; font-size: 13px; color: var(--text-2); text-align: left; min-width: 0; }
  .cn { display: flex; flex-direction: column; min-width: 0; }
  .cs { color: var(--muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chip:hover { border-color: var(--line-strong); color: var(--text); }
  .chip.on { border-color: var(--accent); background: var(--accent-bg); color: var(--text); }
  .help { display: flex; flex-direction: column; gap: 8px; border: 1px solid var(--line); border-radius: var(--r-lg); padding: 8px; }
  .small { font-size: 12px; padding: 0 8px 4px; }
  .kinds { display: flex; flex-wrap: wrap; gap: 2px 4px; align-items: center; }
  .kinds .small { padding: 0 6px 0 2px; }
  .kind { padding: 4px 8px; border-radius: var(--r-md); font-size: 13px; color: var(--text-2); }
  .kind:hover { background: var(--surface-hover); }
  .kind.on { background: var(--surface-hover); color: var(--text); }
  .stacks { max-height: 252px; overflow-y: auto; }
  .stack { display: grid; grid-template-columns: 22px 1fr auto; gap: 2px 10px; align-items: start; padding: 8px 6px; border-top: 1px solid var(--line); }
  .stack:first-child { border-top: 0; padding-top: 2px; }
  .stack :global(svg) { margin-top: 1px; }
  .stack .t { color: var(--text); font-size: 13px; }
  .stack .t .m { font-size: 12px; margin-left: 6px; }
  .stack .g2 { grid-column: 2 / 4; font-size: 12px; color: var(--text-3); line-height: 1.5; }
  .stack .g2 b { font-weight: 400; color: var(--green); }
  .stack .g2 i { font-style: normal; color: var(--amber); }
  .tag { font-size: 11px; color: var(--accent); background: var(--accent-bg); border-radius: var(--r-sm); padding: 1px 6px; margin-left: 8px; }
  .use { font-size: 12px; }
</style>
