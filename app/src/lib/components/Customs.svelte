<!-- Settings > Customs: everything from outside that agents build with, and what happens to it. At the top the numbers and the last week, a line on what Customs is doing right now, then tabs: the log of what happened (customs_log.rs), what waits for you, the list by status, how it runs and adding by hand. A row opens its item on the right: what the review said, its own story, and the buttons. What clears has the stamp for every agent on every machine; only what cannot be verified waits here for you.
     Used by Settings.svelte. -->
<script lang="ts">
  import { poll as every } from "../poll";
  import Quiet from "./Quiet.svelte";
  import Tabs from "./Tabs.svelte";
  // Settings > Customs: everything from outside that agents build with, and what happens to it.
  // At the top the numbers and the last week, a line on what Customs is doing right now, then
  // tabs: the log of what happened (customs_log.rs), what waits for you, the list by status, how
  // it runs and adding by hand. A row opens its item on the right: what the review said, its
  // own story, and the buttons. What clears has the stamp for every agent on every machine;
  // only what cannot be verified waits here for you.
  import { onDestroy, onMount } from "svelte";
  import { api } from "../api";
  import type { CustomsItem } from "../bindings/CustomsItem";
  import type { CustomsEdit } from "../bindings/CustomsEdit";
  import type { CustomsStatus } from "../bindings/CustomsStatus";
  import type { CustomsPrefs } from "../bindings/CustomsPrefs";
  import type { CustomsRegistry } from "../bindings/CustomsRegistry";
  import type { CustomsActivity } from "../bindings/CustomsActivity";
  import type { CustomsLogEntry } from "../bindings/CustomsLogEntry";
  import Switch from "./Switch.svelte";
  import Icon from "./Icon.svelte";
  import { ago, folderName, fullTime } from "../format";
  import { hub } from "../state.svelte";
  import Select from "./Select.svelte";
  import Ref from "./Ref.svelte";
  import { cameThrough, dayLabel, headline, hm, isToday, perDay, ring, took } from "../customsLog";

  const KINDS = ["npm", "crates", "pypi", "cdn", "cli", "mcp", "docker", "service"];
  const statusLabel: Record<CustomsStatus, string> = { cleared: "Cleared", held: "Held", refused: "Refused" };
  const by: Record<string, string> = { existing: "already in use", you: "by you", review: "by review", declared: "declared by an agent", found: "brought in by a thread" };

  type Tab = "log" | "held" | "cleared" | "refused" | "advisories" | "runs" | "add";

  let items = $state<CustomsItem[] | null>(null);
  let activity = $state<CustomsActivity | null>(null);
  let tab = $state<Tab>("log");
  let query = $state("");
  let kind = $state("");
  let picked = $state<{ kind: string; name: string } | null>(null);
  let story = $state<CustomsLogEntry[] | null>(null);
  let newKind = $state("npm");
  let newName = $state("");
  let folder = $state("");
  let importing = $state(false);
  let imported = $state<string | null>(null);
  let prefs = $state<CustomsPrefs | null>(null);
  let registry = $state<CustomsRegistry | null>(null);
  let reviewing = $state<Set<string>>(new Set());
  let copied = $state(false);
  let now = $state(Date.now());

  async function load() {
    try {
      items = await api.customs();
    } catch (e) {
      hub.fail(e);
    }
  }
  async function watch() {
    try {
      activity = await api.customsActivity();
    } catch {
      // An older core has no log yet; the list still works.
    }
  }
  async function readStory() {
    if (!picked) return (story = null);
    const want = picked;
    try {
      const a = await api.customsActivity(want.kind, want.name);
      if (picked === want) story = a.entries;
    } catch {
      story = [];
    }
  }

  // What happens is asked for again every few seconds while the page is open, more often while a
  // review runs, so the line at the top and the log keep up on their own.
  let timer: ReturnType<typeof setTimeout> | null = null;
  let tick: (() => void) | null = null;
  function poll() {
    const busy = (activity?.reviewing.length ?? 0) > 0;
    timer = setTimeout(async () => {
      const before = activity?.entries[0]?.id;
      await watch();
      if (activity?.entries[0]?.id !== before) {
        await load();
        await readStory();
      }
      poll();
    }, busy ? 2000 : 8000);
  }
  onMount(() => {
    load();
    watch().then(poll);
    api.customsPrefs().then((p) => (prefs = p), hub.fail);
    api.customsRegistry().then((r) => (registry = r), hub.fail);
    tick = every(() => (now = Date.now()), 1000);
  });
  onDestroy(() => {
    if (timer) clearTimeout(timer);
    if (tick) tick();
  });

  function pick(k: string, n: string) {
    picked = picked?.kind === k && picked.name === n ? null : { kind: k, name: n };
    story = null;
    readStory();
  }

  async function setPref(change: Partial<CustomsPrefs>) {
    if (!prefs) return;
    try {
      prefs = await api.setCustomsPrefs({ ...prefs, ...change });
    } catch (e) {
      hub.fail(e);
    }
  }

  function put(saved: CustomsItem) {
    if (!items) return;
    const at = items.findIndex((x) => x.kind === saved.kind && x.name === saved.name);
    if (at >= 0) items[at] = saved;
    else items = [...items, saved].sort((a, b) => a.kind.localeCompare(b.kind) || a.name.localeCompare(b.name));
  }

  async function review(i: CustomsItem) {
    const key = i.kind + "/" + i.name;
    reviewing = new Set([...reviewing, key]);
    watch();
    try {
      const saved = await api.reviewCustoms(i.kind, i.name);
      if (saved) put(saved);
    } catch (e) {
      hub.fail(e);
    } finally {
      reviewing = new Set([...reviewing].filter((k) => k !== key));
      await watch();
      await readStory();
    }
  }

  async function copyCargo() {
    if (!registry) return;
    await navigator.clipboard.writeText(registry.cargo_config);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  async function edit(e: CustomsEdit) {
    try {
      put(await api.editCustoms(e));
      if (e.status) {
        await watch();
        await readStory();
      }
    } catch (err) {
      hub.fail(err);
    }
  }

  const stamp = (i: CustomsItem, status: CustomsStatus) => edit({ kind: i.kind, name: i.name, status });

  function saveText(i: CustomsItem, field: "category" | "summary", value: string) {
    if (value.trim() !== i[field]) edit({ kind: i.kind, name: i.name, [field]: value.trim() });
  }

  async function remove(i: CustomsItem) {
    try {
      await api.removeCustoms(i.kind, i.name);
      items = (items ?? []).filter((x) => !(x.kind === i.kind && x.name === i.name));
      picked = null;
      await watch();
    } catch (e) {
      hub.fail(e);
    }
  }

  async function add() {
    const name = newName.trim();
    if (!name || !newKind.trim()) return;
    await edit({ kind: newKind.trim(), name, status: "cleared" });
    newName = "";
  }

  async function importFolder() {
    if (!folder) return;
    importing = true;
    try {
      const r = await api.importCustoms(folder);
      imported = r.added ? `Cleared ${r.added} already in use${r.known ? `, ${r.known} known` : ""}.` : `Nothing new; ${r.known} known.`;
      await load();
      await watch();
    } catch (e) {
      hub.fail(e);
    } finally {
      importing = false;
    }
  }

  function openThread(id: string) {
    hub.settingsOpen = false;
    hub.open(id);
  }

  const all = $derived(items ?? []);
  const count = (s: CustomsStatus) => all.filter((i) => i.status === s).length;
  const held = $derived(all.filter((i) => i.status === "held"));
  const advised = $derived(all.filter((i) => i.status !== "refused" && i.advisories.length));
  const log = $derived(activity?.entries ?? []);
  const today = $derived(log.filter((e) => isToday(e.ts, now)));
  const week = $derived(perDay(log, 7, now));
  const peak = $derived(Math.max(1, ...week.map((d) => d.n)));
  const reviews = $derived(log.filter((e) => e.by === "review" && e.took_ms != null));
  const average = $derived(reviews.length ? reviews.reduce((n, e) => n + (e.took_ms ?? 0), 0) / reviews.length : null);
  const lastCheck = $derived(log.find((e) => e.what === "daily_check"));
  const summary = $derived.by(() => {
    const came = today.filter(cameThrough).length;
    const decided = (w: string) => today.filter((e) => e.what === w && e.by !== "you").length;
    const wait = held.length ? `${held.length} ${held.length === 1 ? "waits" : "wait"} for you. ` : "";
    if (!activity) return wait;
    if (!came) return `${wait}Nothing new came through today.`;
    return `${wait}Today ${came} came through: ${decided("cleared")} cleared by review, ${decided("held")} held, ${decided("refused")} refused.`;
  });

  const words = $derived(query.toLowerCase().split(/\s+/).filter(Boolean));
  const matches = (text: string) => words.every((w) => text.toLowerCase().includes(w));
  const kinds = $derived([...new Set([...all.map((i) => i.kind), ...log.map((e) => e.kind).filter(Boolean)])].sort());
  const shownLog = $derived(
    log.filter((e) => (!kind || e.kind === kind) && matches(`${e.name} ${e.detail} ${e.thread_title ?? ""} ${e.cwd ?? ""} ${headline(e)}`)),
  );
  const shownItems = $derived.by(() => {
    const of =
      tab === "held" ? held : tab === "advisories" ? advised : tab === "refused" ? all.filter((i) => i.status === "refused") : all.filter((i) => i.status === "cleared");
    return of.filter((i) => (!kind || i.kind === kind) && matches(`${i.name} ${i.category} ${i.summary}`));
  });
  const days = $derived.by(() => {
    const out: { label: string; rows: CustomsLogEntry[] }[] = [];
    for (const e of shownLog) {
      const label = dayLabel(e.ts, now);
      if (out.at(-1)?.label !== label) out.push({ label, rows: [] });
      out.at(-1)!.rows.push(e);
    }
    return out;
  });
  const item = $derived(picked ? all.find((i) => i.kind === picked!.kind && i.name === picked!.name) ?? null : null);
  const isPicked = (k: string, n: string) => picked?.kind === k && picked.name === n;
  const listTab = $derived(tab === "log" || tab === "held" || tab === "cleared" || tab === "refused" || tab === "advisories");

  /** The line under an item's name: where it stands and since when. */
  function standing(i: CustomsItem): string {
    if (i.status === "held") return `Held for you since ${fullTime(i.updated_at)}`;
    const who = by[i.decided_by] ?? i.decided_by;
    return `${statusLabel[i.status]} ${who}${i.reviewed_at ? ` · last reviewed ${ago(i.reviewed_at, now)} ago` : ""}`;
  }

  const TABS: [Tab, string][] = [
    ["log", "What happened"],
    ["held", "Needs you"],
    ["cleared", "Cleared"],
    ["refused", "Refused"],
    ["advisories", "Advisories"],
  ];
  const tabCount = (t: Tab) =>
    t === "log" ? log.length : t === "held" ? held.length : t === "cleared" ? count("cleared") : t === "refused" ? count("refused") : advised.length;
</script>

<h1>Customs</h1>
<p class="lead">
  What agents bring in.
  {summary}
</p>

<div class="strip">
  <div class="stat"><span class="v">{count("cleared")}</span><span class="l">cleared</span></div>
  <button class="stat" onclick={() => (tab = "held")}><span class="v" class:amber={held.length}>{held.length}</span><span class="l">held</span></button>
  <button class="stat" onclick={() => (tab = "refused")}><span class="v">{count("refused")}</span><span class="l">refused</span></button>
  <button class="stat" onclick={() => (tab = "advisories")}><span class="v">{advised.length}</span><span class="l">{advised.length === 1 ? "advisory" : "advisories"}</span></button>
  {#if average != null}<div class="stat" data-tip="Average over 30 days"><span class="v">{took(average)}</span><span class="l">a review</span></div>{/if}
  <div class="stat" data-tip="Last 7 days. Amber: held.">
    <span class="bars">{#each week as d, i (i)}<i class:amber={d.held} style:height="{d.n ? 18 + (d.n / peak) * 82 : 8}%"></i>{/each}</span>
    <span class="l">came through, 7 days</span>
  </div>
</div>

<div class="now">
  {#if activity?.reviewing.length}
    {@const r = activity.reviewing[0]}
    <span class="pulse"></span>
    <span>Now: reviewing</span>
    <button class="mono linkish" onclick={() => pick(r.kind, r.name)}>{r.name}</button>
    <span class="muted">{r.kind}</span>
    {#if activity.reviewing.length > 1}<span class="muted">and {activity.reviewing.length - 1} more</span>{/if}
    <span class="t">{Math.max(0, Math.round((now - r.since) / 1000))} s</span>
  {:else}
    <span class="still"></span>
    <span class="muted">
      Quiet now.{#if log[0]}&nbsp;Last: {headline(log[0]).toLowerCase()}{log[0].name ? ` ${log[0].name}` : ""}, {ago(log[0].ts, now)} ago.{/if}
      {#if lastCheck}&nbsp;Daily check {ago(lastCheck.ts, now)} ago.{:else if prefs && !prefs.daily_check}&nbsp;The daily check is off.{/if}
    </span>
  {/if}
</div>

<Tabs style="margin: 20px 0 12px" value={tab} onchange={(id) => (tab = id)} items={[
  ...TABS.map(([t, label]) => ({ id: t as Tab, label, count: tabCount(t), amber: t === "held" && held.length > 0 })),
  { id: "runs" as Tab, label: "How it runs", end: true },
  { id: "add" as Tab, label: "Add" },
]} />

{#if listTab}
  <div class="two" class:open={!!picked}>
    <div class="side">
      <div class="filters">
        <input class="field search" bind:value={query} placeholder={tab === "log" ? "Search name, thread or project" : "Search name, category or summary"} spellcheck="false" />
        <Select size="small" label="Kind" bind:value={kind} options={[{ value: "", label: "All kinds" }, ...kinds.map((k) => ({ value: k, label: k }))]} />
      </div>

      {#if tab === "log"}
        {#each days as d (d.label)}
          <div class="day">{d.label}</div>
          {#each d.rows as e (e.id)}
            <div
              class="ev"
              class:sel={!!e.name && isPicked(e.kind, e.name)}
              class:item={!!e.name}
              role="button"
              tabindex={e.name ? 0 : -1}
              onclick={() => e.name && pick(e.kind, e.name)}
              onkeydown={(k) => k.key === "Enter" && e.name && pick(e.kind, e.name)}
            >
              <span class="t" data-tip={fullTime(e.ts)}>{hm(e.ts)}</span>
              <span class="ring {ring(e)}"></span>
              <span class="kind">{e.kind}</span>
              <span class="name" class:mono={!!e.name}>{e.name || headline(e)}</span>
              <span class="what" data-tip={e.detail}>
                {#if e.name}<b>{headline(e)}</b>{/if}{#if e.thread_id}&nbsp;in <button class="linkish" data-tip="Open the thread" onclick={(ev) => (ev.stopPropagation(), openThread(e.thread_id!))}>{e.thread_title || "a thread"}</button>{/if}{#if e.took_ms != null}&nbsp;· took {took(e.took_ms)}{/if}{#if e.detail}{e.name ? " · " : ""}{e.detail}{/if}
                {#if e.cwd}· <Ref kind="project" project={{ path: e.cwd }} label={folderName(e.cwd)} title={e.cwd} />{/if}
              </span>
            </div>
          {/each}
        {:else}
          <div class="empty">
            {#if !activity}Reading…{:else if log.length}Nothing matches.{:else}Nothing yet.{/if}
          </div>
        {/each}
      {:else}
        {#each shownItems as i (i.kind + i.name)}
          <div
            class="row"
            class:sel={isPicked(i.kind, i.name)}
            role="button"
            tabindex="0"
            onclick={() => pick(i.kind, i.name)}
            onkeydown={(k) => k.key === "Enter" && pick(i.kind, i.name)}
          >
            <span class="ring {i.status === 'cleared' ? 'ok' : i.status === 'held' ? 'warn' : 'bad'}"></span>
            <span class="kind">{i.kind}</span>
            <span class="name mono" class:struck={i.status === "refused"}>{i.name}</span>
            {#if tab === "held"}
              <span class="what" data-tip={i.reason}>{i.summary ? `${i.summary}. ` : ""}{i.reason}</span>
              <span class="acts">
                <button class="btn small" disabled={reviewing.has(i.kind + "/" + i.name)} onclick={(e) => (e.stopPropagation(), review(i))}>{reviewing.has(i.kind + "/" + i.name) ? "Reviewing…" : "Review again"}</button>
                <button class="btn small primary" onclick={(e) => (e.stopPropagation(), stamp(i, "cleared"))}>Clear</button>
                <button class="btn small" onclick={(e) => (e.stopPropagation(), stamp(i, "refused"))}>Refuse</button>
              </span>
            {:else}
              <span class="cat">{i.category}</span>
              <span class="what">{tab === "advisories" ? i.advisories.join(", ") : tab === "refused" ? i.reason : i.summary}</span>
              {#if i.advisories.length && tab !== "advisories"}<span class="pill warn">{i.advisories.length} {i.advisories.length === 1 ? "advisory" : "advisories"}</span>{/if}
            {/if}
          </div>
        {:else}
          <div class="empty">
            {#if !items}Reading…{:else if tab === "held"}Nothing waits for you.{:else if all.length}Nothing here.{:else}Nothing yet.{/if}
          </div>
        {/each}
      {/if}
    </div>

    {#if picked}
      <aside class="pane">
        <div class="pane-head">
          <span class="kind">{picked.kind}</span>
          <button class="close" aria-label="Close" onclick={() => (picked = null)}><Icon name="x" size={12} /></button>
        </div>
        <h3 class="mono" class:struck={item?.status === "refused"}>{picked.name}</h3>
        {#if item}
          <div class="sub">{standing(item)}</div>
          <div class="acts">
            <button class="btn small" disabled={reviewing.has(item.kind + "/" + item.name)} onclick={() => review(item)}>{reviewing.has(item.kind + "/" + item.name) ? "Reviewing…" : "Review again"}</button>
            {#if item.status !== "cleared"}<button class="btn small primary" onclick={() => stamp(item, "cleared")}>Clear</button>{/if}
            {#if item.status !== "refused"}<button class="btn small" data-tip="Agents can't use it" onclick={() => stamp(item, "refused")}>Refuse</button>{/if}
            <button class="btn small" data-tip="Off the list; agents can declare it again" onclick={() => remove(item)}>Remove</button>
          </div>

          {#if item.reason}
            <div class="sec">Why</div>
            <p class="say">{item.reason}</p>
          {/if}
          {#if item.advisories.length}
            <div class="sec">Known vulnerabilities in its newest version</div>
            <p class="say warn">{item.advisories.join(", ")}</p>
          {/if}

          <div class="sec">What it is</div>
          <div class="fields">
            <input class="inline" value={item.category} placeholder="Category" spellcheck="false"
              onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              onblur={(e) => saveText(item, "category", e.currentTarget.value)} />
            <input class="inline" value={item.summary} placeholder="What it is for, in one line" spellcheck="false"
              onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              onblur={(e) => saveText(item, "summary", e.currentTarget.value)} />
          </div>
        {:else}
          <div class="sub">No longer on the list.</div>
        {/if}

        <div class="sec">What happened</div>
        {#if story === null}
          <Quiet kind="loading" />
        {:else if !story.length}
          <p class="say muted">Nothing logged yet{item ? ` since it came on the list ${ago(item.created_at, now)} ago` : ""}.</p>
        {:else}
          <div class="tl">
            {#each story as e (e.id)}
              <div class="step {ring(e)}">
                <span class="t" data-tip={fullTime(e.ts)}>{isToday(e.ts, now) ? hm(e.ts) : dayLabel(e.ts, now)}</span>
                <span>
                  {headline(e)}{#if e.took_ms != null}&nbsp;· took {took(e.took_ms)}{/if}{#if e.thread_id}&nbsp;in <button class="linkish" data-tip="Open the thread" onclick={(ev) => (ev.stopPropagation(), openThread(e.thread_id!))}>{e.thread_title || "a thread"}</button>{/if}
                  {#if e.detail && e.detail !== item?.reason}<span class="muted"> · {e.detail}</span>{/if}
                </span>
              </div>
            {/each}
          </div>
        {/if}
      </aside>
    {/if}
  </div>
{:else if tab === "runs"}
  <section>
    {#if prefs}
      <div class="line">
        <div class="what-l">
          <div class="name-l">Review what is new by itself</div>
          <div class="hint">Green clears; the rest waits for you.</div>
        </div>
        <div class="control"><Switch label="Review what is new by itself" on={prefs.auto_review} onchange={(v) => setPref({ auto_review: v })} /></div>
      </div>
      <div class="line">
        <div class="what-l">
          <div class="name-l">Check the list every day</div>
          <div class="hint">Malware is refused; vulnerabilities are noted.</div>
        </div>
        <div class="control"><Switch label="Check the list every day" on={prefs.daily_check} onchange={(v) => setPref({ daily_check: v })} /></div>
      </div>
      <div class="line">
        <div class="what-l">
          <div class="name-l">Update every month</div>
          <div class="hint">Re-reviews old items and updates dependencies in a thread per project.</div>
        </div>
        <div class="control"><Switch label="Update every month" on={prefs.monthly_update} onchange={(v) => setPref({ monthly_update: v })} /></div>
      </div>
      <div class="line">
        <div class="what-l">
          <div class="name-l">Install through Customs</div>
          <div class="hint">
            npm, yarn, pip and uv fetch through <span class="mono">{registry?.base ?? "…"}</span>. New items are reviewed; nothing under a week old.
            {#if registry}<br />Cargo needs this in <span class="mono">~/.cargo/config.toml</span>: <button class="link" onclick={copyCargo}>{copied ? "Copied" : "Copy"}</button>{/if}
          </div>
        </div>
        <div class="control"><Switch label="Install through Customs" on={prefs.registry} onchange={(v) => setPref({ registry: v })} /></div>
      </div>
    {/if}
  </section>
{:else}
  <section>
    <div class="line">
      <div class="what-l">
        <div class="name-l">Clear something yourself</div>
        <div class="hint">Kind, such as {KINDS.join(", ")}.</div>
      </div>
      <div class="control">
        <input class="field kindin" bind:value={newKind} list="customs-kinds" spellcheck="false" />
        <datalist id="customs-kinds">{#each KINDS as k (k)}<option value={k}></option>{/each}</datalist>
        <input class="field" bind:value={newName} placeholder="Name, e.g. bootstrap" spellcheck="false" onkeydown={(e) => e.key === "Enter" && add()} />
        <button class="btn primary" disabled={!newName.trim()} onclick={add}>Clear</button>
      </div>
    </div>
    <div class="line">
      <div class="what-l">
        <div class="name-l">Read a project's packages</div>
        <div class="hint">{imported ?? "Clears its direct dependencies."}</div>
      </div>
      <div class="control">
        <Select bind:value={folder} label="Project" placeholder="Pick a project" options={hub.projects.map((p) => ({ value: p.path, label: p.name }))} />
        <button class="btn" disabled={!folder || importing} onclick={importFolder}>{importing ? "Reading…" : "Read"}</button>
      </div>
    </div>
  </section>
{/if}

<style>
  h1 { color: var(--text); font-size: 20px; font-weight: 600; margin: 0 0 6px; }
  .lead { color: var(--text-3); margin: 0 0 14px; max-width: var(--prose); }
  .muted { color: var(--muted); }
  .mono { font-family: var(--mono); }

  .strip { display: flex; gap: 24px; align-items: flex-end; flex-wrap: wrap; }
  .stat { display: flex; flex-direction: column; align-items: flex-start; gap: 1px; padding: 0; background: none; text-align: left; }
  button.stat:hover .l { color: var(--text-3); }
  .stat .v { color: var(--text); font-size: 18px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .stat .v.amber { color: var(--amber); }
  .stat .l { color: var(--muted); font-size: 12px; }
  .bars { display: flex; align-items: flex-end; gap: 3px; height: 26px; }
  .bars i { width: 9px; background: var(--line-strong); border-radius: 2px; display: block; }
  .bars i.amber { background: var(--amber); }

  .now { display: flex; align-items: center; gap: 8px; margin: 14px 0 0; padding: 8px 12px; border: 1px solid var(--line); border-radius: var(--r-md); background: var(--surface); font-size: 13px; color: var(--text-3); min-width: 0; }
  .now .t { margin-left: auto; color: var(--muted); font-variant-numeric: tabular-nums; }
  .now .muted { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pulse, .still { width: 8px; height: 8px; border-radius: 50%; flex: none; }
  .pulse { background: var(--green); animation: breathe 1.6s var(--ease-move) infinite; }
  .still { border: 1.5px solid var(--muted); box-sizing: border-box; }
  @keyframes breathe { 50% { opacity: 0.35; } }
  .linkish { color: var(--text-2); background: none; padding: 0; font-size: 13px; height: auto; display: inline; }
  .linkish:hover { text-decoration: underline; text-underline-offset: 3px; text-decoration-color: var(--line-strong); }


  .two { display: grid; grid-template-columns: minmax(0, 1fr); gap: 22px; }
  .two.open { grid-template-columns: minmax(0, 1fr) minmax(260px, 340px); }
  .side { min-width: 0; }
  .filters { display: flex; gap: 8px; align-items: center; margin: 0 0 6px; }
  .field { height: 28px; padding: 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); font-size: 13px; outline: none; min-width: 0; }
  .field:focus { border-color: var(--accent); }
  .search { flex: 1; }
  .kindin { width: 90px; }

  .day { color: var(--muted); font-size: 12px; margin: 16px 0 2px; }
  .ev, .row { display: grid; gap: 10px; align-items: center; padding: 6px 6px; min-height: 34px; border-top: 1px solid var(--line); font-size: 13px; }
  .day + .ev, .filters + .row { border-top-color: transparent; }
  .ev { grid-template-columns: 40px 10px 46px minmax(90px, 160px) minmax(0, 1fr); }
  .row { grid-template-columns: 10px 46px minmax(110px, 190px) 90px minmax(0, 1fr) auto; }
  .ev.item, .row { cursor: pointer; }
  .ev.item:hover, .row:hover { background: color-mix(in srgb, var(--surface-hover) 60%, transparent); }
  .ev.sel, .row.sel { background: var(--surface-hover); border-radius: var(--r-md); border-top-color: transparent; }
  .ev.sel + .ev, .row.sel + .row { border-top-color: transparent; }
  .t { color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; }
  .kind { color: var(--muted); font-size: 12px; }
  .name { color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name.mono { color: var(--text); font-size: 13px; }
  .struck { text-decoration: line-through; color: var(--muted) !important; }
  .cat { color: var(--text-3); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .what { color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .what b { color: var(--text-2); font-weight: 500; }
  .row:has(.acts) { grid-template-columns: 10px 46px minmax(110px, 190px) minmax(0, 1fr) auto; }
  .acts { display: flex; gap: 6px; flex-wrap: wrap; }

  .ring { width: 9px; height: 9px; border-radius: 50%; border: 1.5px solid var(--muted); box-sizing: border-box; flex: none; }
  .ring.ok { border-color: var(--green); background: var(--green); }
  .ring.warn { border-color: var(--amber); }
  .ring.bad { border-color: var(--red); background: var(--red); }
  .ring.round { border-style: dotted; }

  .pill { font-size: 12px; border-radius: var(--r-sm); padding: 1px 8px; white-space: nowrap; flex: none; }
  .pill.warn { color: var(--amber); background: var(--amber-bg); }
  .btn.small { height: 24px; padding: 0 8px; font-size: 12px; flex: none; }

  .pane { border-left: 1px solid var(--line); padding-left: 20px; min-width: 0; align-self: start; position: sticky; top: 0; }
  .pane-head { display: flex; align-items: center; }
  .close { margin-left: auto; color: var(--muted); padding: 4px; border-radius: var(--r-sm); background: none; }
  .close:hover { color: var(--text); background: var(--surface-hover); }
  .pane h3 { font-size: 16px; color: var(--text); margin: 2px 0 2px; font-weight: 500; overflow-wrap: anywhere; }
  .sub { color: var(--muted); font-size: 13px; margin-bottom: 12px; }
  .sec { color: var(--muted); font-size: 12px; margin: 18px 0 6px; }
  .say { color: var(--text-3); font-size: 13px; margin: 0; overflow-wrap: anywhere; }
  .say.warn { color: var(--amber); }
  .fields { display: flex; flex-direction: column; gap: 4px; }
  .inline { height: 26px; padding: 0 8px; background: transparent; border: 1px solid var(--line); border-radius: var(--r-md); font-size: 13px; color: var(--text-2); outline: none; min-width: 0; }
  .inline:focus { border-color: var(--accent); background: var(--bg); }
  .inline::placeholder { color: var(--muted); opacity: 0.7; }
  .tl { position: relative; padding-left: 18px; }
  .tl::before { content: ""; position: absolute; left: 4px; top: 8px; bottom: 8px; width: 1px; background: var(--line-strong); }
  .step { position: relative; display: flex; gap: 8px; padding: 4px 0; font-size: 13px; color: var(--text-3); }
  .step::before { content: ""; position: absolute; left: -18px; top: 8px; width: 9px; height: 9px; border-radius: 50%; background: var(--bg); border: 1.5px solid var(--muted); box-sizing: border-box; }
  .step.ok::before { border-color: var(--green); background: var(--green); }
  .step.warn::before { border-color: var(--amber); }
  .step.bad::before { border-color: var(--red); background: var(--red); }
  .step.round::before { border-style: dotted; }
  .step .t { flex: none; min-width: 40px; }

  .line { display: flex; align-items: center; gap: 32px; padding: 16px 0; }
  .line + .line { border-top: 1px solid var(--line); }
  .what-l { flex: 1; min-width: 0; }
  .name-l { color: var(--text); font-size: 14px; }
  .hint { color: var(--muted); font-size: 13px; margin-top: 4px; }
  .hint .link { color: var(--accent); font-size: 13px; padding: 0 4px; }
  .control { display: flex; align-items: center; gap: 8px; flex: none; }
  .empty { color: var(--muted); padding: 12px 0; font-size: 13px; }
</style>
