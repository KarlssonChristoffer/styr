<!-- Settings > Notifications: what may come through on each device, your own notice lines, and when Hub keeps quiet (sketch A: a card per device). The PC's rules live in the window (`notices.ts`); the iPhone's are the phone's own, shown here as it set them.
     Used by Settings.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  // Settings > Notifications: what may come through on each device, your own notice lines, and when
  // Hub keeps quiet (sketch A: a card per device). The PC's rules live in the window (`notices.ts`);
  // the iPhone's are the phone's own, shown here as it set them.
  import { onMount } from "svelte";
  import { api, type PhoneNotify } from "../api";
  import { hubdHas } from "../compat";
  import { folderName, isChatThread, projectFolder, threadProject } from "../format";
  import { AFTERS, HOWS, SORTS, comingThrough, sortOn, testNotice } from "../notices";
  import { prefs, setPref, type NoticeLine, type NoticeSort } from "../prefs.svelte";
  import { hub } from "../state.svelte";
  import { isSystem } from "../system";
  import Icon from "./Icon.svelte";
  import Select, { type Option } from "./Select.svelte";
  import Switch from "./Switch.svelte";

  const SWITCH_KEY = { waiting: "notifyWaiting", failed: "notifyFailed", done: "notifyDone" } as const;
  const SIGN: Record<NoticeSort, { glyph: string; tone: string }> = {
    waiting: { glyph: "?", tone: "amber" },
    failed: { glyph: "✕", tone: "red" },
    done: { glyph: "✓", tone: "violet" },
  };

  /** Every project a notice can be limited to: the folders added as projects and the ones threads run in. */
  const projects = $derived.by<Option<string>[]>(() => {
    const found = new Map<string, string>();
    for (const p of hub.projects) found.set(projectFolder(p.path), p.name);
    for (const t of hub.everything) {
      if (isSystem(t) || isChatThread(t)) continue;
      const f = threadProject(t);
      if (!found.has(f)) found.set(f, folderName(f));
    }
    const named = [...found].map(([value, label]) => ({ value, label })).sort((a, b) => a.label.localeCompare(b.label));
    return [{ value: "", label: "all projects" }, ...named];
  });

  const sortOptions: Option<NoticeSort>[] = SORTS.map((s) => ({ value: s.id, label: s.sentence }));
  const howOptions: Option<NoticeLine["how"]>[] = HOWS;
  const afterOptions: Option<number>[] = AFTERS;
  const hourOptions: Option<number>[] = Array.from({ length: 24 }, (_, h) => ({ value: h, label: `${String(h).padStart(2, "0")}:00` }));
  const soundOptions: Option<"off" | "soft" | "bell">[] = [
    { value: "off", label: "Off" },
    { value: "soft", label: "Soft" },
    { value: "bell", label: "Bell" },
  ];

  const q = $derived(prefs.noticeQuiet);
  const setQuiet = (patch: Partial<typeof prefs.noticeQuiet>) => setPref("noticeQuiet", { ...prefs.noticeQuiet, ...patch });
  const scopeOf = (sort: NoticeSort) => prefs.noticeScope[sort] ?? "";
  const setScope = (sort: NoticeSort, v: string) => setPref("noticeScope", { ...prefs.noticeScope, [sort]: v || null });

  function addLine() {
    const line: NoticeLine = { id: crypto.randomUUID().slice(0, 8), sort: "waiting", project: null, how: "notice", after: 15 };
    setPref("noticeLines", [...prefs.noticeLines, line]);
  }
  const patchLine = (id: string, patch: Partial<NoticeLine>) =>
    setPref("noticeLines", prefs.noticeLines.map((l) => (l.id === id ? { ...l, ...patch } : l)));
  const dropLine = (id: string) => setPref("noticeLines", prefs.noticeLines.filter((l) => l.id !== id));

  // The pause: a few common lengths, and a button to end it.
  let now = $state(Date.now());
  $effect(() => {
    const t = poll(() => (now = Date.now()), 20_000);
    return () => t();
  });
  const paused = $derived(q.pausedUntil > now);
  const at = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
  function pauseFor(minutes: number) {
    setQuiet({ pausedUntil: Date.now() + minutes * 60_000 });
    now = Date.now();
  }
  function pauseTillMorning() {
    const d = new Date();
    d.setHours(8, 0, 0, 0);
    if (d.getTime() <= Date.now()) d.setDate(d.getDate() + 1);
    setQuiet({ pausedUntil: d.getTime() });
    now = Date.now();
  }

  let pcTest = $state("");
  let pcTesting = $state(false);
  async function testPc() {
    pcTesting = true;
    pcTest = await testNotice();
    pcTesting = false;
  }

  // The phone: what it asked this machine to send, if it has asked.
  let phone = $state<PhoneNotify | null>(null);
  let phoneKnown = $state(false);
  let phoneTest = $state("");
  onMount(async () => {
    try {
      if (await hubdHas("phone.notify")) phone = await api.phoneNotify(); // compat-until: 2026-12-01
    } catch {
      // This Hub does not know the call yet; the card says so.
    }
    phoneKnown = true;
  });
  const phoneSet = $derived(!!phone && phone.topic.trim() !== "");
  const phoneOn = (sort: NoticeSort) =>
    !phone ? false : sort === "waiting" ? phone.approvals || phone.questions : sort === "failed" ? phone.failures : phone.done;
  async function testPhone() {
    phoneTest = "";
    try {
      await api.phoneNotifyTest();
      phoneTest = "Sent. It arrives in the ntfy app.";
    } catch (err) {
      phoneTest = err instanceof Error ? err.message : String(err);
    }
  }
  const hh = (h: number | null) => (h === null ? "" : `${String(h).padStart(2, "0")}:00`);

  const pcCount = $derived(comingThrough());
</script>

<h1>Notifications</h1>
<p class="lead">Nothing reaches you until you choose it, device by device.</p>

{#if paused}
  <div class="paused"><Icon name="pause" size={14} /><span>Paused until {at(q.pausedUntil)}. Nothing comes through on this PC.</span><button class="btn" onclick={() => setQuiet({ pausedUntil: 0 })}>Resume</button></div>
{/if}

<div class="devices">
  <section class="card">
    <header>
      <span class="ic"><Icon name="monitor" size={18} /></span>
      <div class="who"><div class="name">This PC</div><div class="sub">Windows · this Hub window</div></div>
      <button class="btn" disabled={pcTesting} onclick={testPc}>Send test</button>
    </header>
    <div class="state" class:none={pcCount === 0}><span class="dot"></span>{pcCount === 0 ? "Nothing comes through" : `${pcCount} ${pcCount === 1 ? "thing comes" : "things come"} through`}</div>
    <div class="rows">
      {#each SORTS as s (s.id)}
        <div class="row" class:on={sortOn(s.id)}>
          <span class="sign {SIGN[s.id].tone}">{SIGN[s.id].glyph}</span>
          <span class="label"><span class="l">{s.label}</span>{#if sortOn(s.id)}<span class="scope"><Select size="small" value={scopeOf(s.id)} options={projects} onchange={(v) => setScope(s.id, v)} label={`${s.label}: which projects`} /></span>{:else}<span class="hint">{s.hint}</span>{/if}</span>
          <Switch on={sortOn(s.id)} label={`${s.label} on this PC`} onchange={(v) => setPref(SWITCH_KEY[s.id], v)} />
        </div>
      {/each}
    </div>
    <footer>
      <span class="muted">Shows as a Windows notice</span>
      <button class="chip" class:on={prefs.flashTaskbar} onclick={() => setPref("flashTaskbar", !prefs.flashTaskbar)}>Taskbar flash</button>
      <span class="sound"><span class="muted">Sound</span><Select size="small" value={prefs.sound} options={soundOptions} onchange={(v) => setPref("sound", v)} label="Notice sound" /></span>
    </footer>
    {#if pcTest}<div class="result">{pcTest}</div>{/if}
  </section>

  <section class="card">
    <header>
      <span class="ic"><Icon name="phone" size={18} /></span>
      <div class="who"><div class="name">iPhone</div><div class="sub">Through the ntfy app</div></div>
      <button class="btn" disabled={!phoneSet} onclick={testPhone}>Send test</button>
    </header>
    {#if !phoneKnown}
      <div class="state none"><span class="dot"></span>Reading…</div>
    {:else if !phoneSet}
      <div class="state none"><span class="dot"></span>Not set up</div>
      <p class="blank">{phone ? "Open Hub on the iPhone, Settings > Notifications, to set it up." : "This Hub is too old to send phone notices; it can after its next update."}</p>
    {:else}
      <div class="state"><span class="dot"></span>Set on the phone</div>
      <div class="rows">
        {#each SORTS as s (s.id)}
          <div class="row" class:on={phoneOn(s.id)}>
            <span class="sign {SIGN[s.id].tone}">{SIGN[s.id].glyph}</span>
            <span class="label"><span class="l">{s.label}</span>{#if !phoneOn(s.id)}<span class="hint">{s.hint}</span>{/if}</span>
            <Switch on={phoneOn(s.id)} disabled label={`${s.label} on the iPhone`} onchange={() => {}} />
          </div>
        {/each}
      </div>
      <footer>
        <span class="muted">{phone?.quiet_from != null ? `Quiet ${hh(phone.quiet_from)} – ${hh(phone.quiet_to)}` : "No quiet hours"}</span>
        <span class="muted">{phone?.quiet_at_pc ? "Quiet while I'm at the PC" : ""}</span>
      </footer>
    {/if}
    {#if phoneTest}<div class="result">{phoneTest}</div>{/if}
    {#if phoneSet}<div class="result soft">The phone keeps these; change them in its Settings. They move here when the core owns the rules.</div>{/if}
  </section>
</div>

<h2>Your own lines</h2>
<p class="sub2">On this PC, on top of the switches. A line with a wait only speaks if the thread is still that way by then.</p>
<section class="card lines">
  {#each prefs.noticeLines as l (l.id)}
    <div class="sentence">
      <span class="sign {SIGN[l.sort].tone}">{SIGN[l.sort].glyph}</span>
      <span>Tell me when</span>
      <Select size="small" value={l.sort} options={sortOptions} onchange={(v) => patchLine(l.id, { sort: v })} label="When" />
      <span>in</span>
      <Select size="small" value={l.project ?? ""} options={projects} onchange={(v) => patchLine(l.id, { project: v || null })} label="Project" />
      <span>as</span>
      <Select size="small" value={l.how} options={howOptions} onchange={(v) => patchLine(l.id, { how: v })} label="How" />
      <Select size="small" value={l.after} options={afterOptions} onchange={(v) => patchLine(l.id, { after: v })} label="After how long" />
      <span class="grow"></span>
      <button class="x" aria-label="Remove this line" onclick={() => dropLine(l.id)}><Icon name="x" size={13} /></button>
    </div>
  {/each}
  <div class="add"><button class="chip" onclick={addLine}>+ Add a line</button>{#if !prefs.noticeLines.length}<span class="muted">For example: tell me when a thread needs me in hub, after 15 minutes, as a Windows notice.</span>{/if}</div>
</section>

<h2>Quiet</h2>
<section class="card quiet">
  <div class="qrow">
    <div class="what"><div class="name">While I'm at the PC</div><div class="hint">No notice while Hub is the window you use and you have touched the keys or the mouse in the last 3 minutes.</div></div>
    <Switch on={q.atPc} label="Quiet while I'm at the PC" onchange={(v) => setQuiet({ atPc: v })} />
  </div>
  <div class="qrow">
    <div class="what"><div class="name">Quiet hours</div><div class="hint">Done and failed wait. What needs you still comes through{q.strict ? ", except that you asked to hold that too" : ""}.</div></div>
    <div class="ctl">
      {#if q.hours}<Select size="small" value={q.from} options={hourOptions} onchange={(v) => setQuiet({ from: v })} label="Quiet from" /><span class="muted">to</span><Select size="small" value={q.to} options={hourOptions} onchange={(v) => setQuiet({ to: v })} label="Quiet until" />{/if}
      <Switch on={q.hours} label="Quiet hours" onchange={(v) => setQuiet({ hours: v })} />
    </div>
  </div>
  {#if q.hours}
    <div class="qrow sub">
      <div class="what"><div class="name">Hold what needs me too</div><div class="hint">Nothing at all in quiet hours, not even a question.</div></div>
      <Switch on={q.strict} label="Hold what needs me in quiet hours" onchange={(v) => setQuiet({ strict: v })} />
    </div>
  {/if}
  <div class="qrow">
    <div class="what"><div class="name">Pause</div><div class="hint">{paused ? `Paused until ${at(q.pausedUntil)}.` : "Nothing for a while, then back as before."}</div></div>
    <div class="ctl">
      {#if paused}<button class="btn" onclick={() => setQuiet({ pausedUntil: 0 })}>Resume</button>
      {:else}<button class="btn" onclick={() => pauseFor(60)}>1 hour</button><button class="btn" onclick={() => pauseFor(240)}>4 hours</button><button class="btn" onclick={pauseTillMorning}>Until 08:00</button>{/if}
    </div>
  </div>
  <div class="qrow">
    <div class="what">
      <div class="name">Focus mode</div>
      <div class="hint">
        {#if prefs.focusOn}On for {prefs.focusOn.name}: only it shows, and {prefs.held.length ? `${prefs.held.length} ${prefs.held.length === 1 ? "notification" : "notifications"} from other projects wait` : "other projects' notifications wait"} until you leave.
        {:else}Only one project shows, and the others' notifications wait until you leave. Turn it on from a project's page or its menu in the sidebar.{/if}
      </div>
    </div>
    {#if prefs.focusOn}<button class="btn" onclick={() => hub.leaveFocus()}>Leave focus</button>{/if}
  </div>
</section>
<p class="sub2 more">Pull requests, updates, plan limits and things from outside come once the core owns the rules, and so does holding back in full-screen apps.</p>

<style>
  h1 { font-size: 22px; font-weight: 600; margin: 0 0 6px; }
  h2 { font-size: 15px; font-weight: 600; margin: 28px 0 4px; }
  .lead { color: var(--muted); margin: 0 0 20px; }
  .sub2 { color: var(--muted); font-size: 13px; margin: 0 0 10px; }
  .sub2.more { margin-top: 14px; }
  .muted { color: var(--muted); font-size: 12px; }
  .paused { display: flex; align-items: center; gap: 10px; padding: 8px 14px; margin: -4px 0 16px; border-radius: var(--r-lg); background: var(--amber-bg); color: var(--amber); font-size: 13px; }
  .paused span { flex: 1; }
  .devices { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
  .card { background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); overflow: hidden; }
  header { display: flex; align-items: center; gap: 10px; padding: 14px 16px 10px; }
  .ic { width: 34px; height: 34px; border-radius: var(--r-md); background: var(--surface-hover); display: grid; place-items: center; color: var(--text-2); flex: none; }
  .who { flex: 1; min-width: 0; }
  .name { font-size: 15px; color: var(--text); font-weight: 500; }
  .sub { color: var(--muted); font-size: 12px; }
  .state { display: flex; align-items: center; gap: 8px; padding: 0 16px 6px; font-size: 13px; color: var(--green); }
  .state .dot { width: 7px; height: 7px; border-radius: 50%; background: currentColor; }
  .state.none { color: var(--muted); }
  .blank { margin: 0; padding: 4px 16px 16px; color: var(--muted); font-size: 13px; }
  .rows { padding: 4px 8px 8px; }
  .row { display: flex; align-items: center; gap: 10px; padding: 8px; border-radius: var(--r-md); }
  .row.on { background: var(--surface-hover); }
  .label { flex: 1; display: flex; align-items: center; gap: 8px; min-width: 0; font-size: 14px; color: var(--text-3); }
  .row.on .l { color: var(--text); }
  .hint { color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sign { width: 18px; height: 18px; border-radius: 50%; display: grid; place-items: center; font-size: 11px; font-weight: 600; flex: none; }
  .sign.amber { color: var(--amber); background: var(--amber-bg); }
  .sign.red { color: var(--red); background: var(--red-bg); }
  .sign.violet { color: var(--p-violet); background: rgba(169, 156, 240, 0.14); }
  footer { display: flex; align-items: center; gap: 10px; padding: 11px 16px; border-top: 1px solid var(--line); flex-wrap: wrap; }
  footer .sound { display: flex; align-items: center; gap: 6px; margin-left: auto; }
  .chip { font-size: 12px; padding: 3px 10px; border-radius: 999px; border: 1px solid var(--line-strong); color: var(--text-3); background: transparent; }
  .chip.on { background: var(--accent-bg); border-color: transparent; color: var(--accent); }
  .chip:hover { color: var(--text); }
  .result { padding: 8px 16px; border-top: 1px solid var(--line); font-size: 13px; color: var(--text-2); }
  .result.soft { color: var(--muted); font-size: 12px; }
  .lines .sentence { display: flex; align-items: center; gap: 8px; padding: 10px 16px; border-bottom: 1px solid var(--line); color: var(--text-3); font-size: 14px; flex-wrap: wrap; }
  .grow { flex: 1; }
  .x { color: var(--muted); padding: 4px; border-radius: var(--r-sm); }
  .x:hover { color: var(--text); background: var(--surface-hover); }
  .add { display: flex; align-items: center; gap: 12px; padding: 11px 16px; }
  .qrow { display: flex; align-items: center; gap: 16px; padding: 12px 16px; border-bottom: 1px solid var(--line); }
  .qrow:last-child { border-bottom: 0; }
  .qrow.sub { padding-left: 32px; background: rgba(255, 255, 255, 0.015); }
  .qrow .what { flex: 1; min-width: 0; }
  .qrow .name { font-size: 14px; color: var(--text-2); font-weight: 400; }
  .qrow .hint { white-space: normal; margin-top: 2px; }
  .ctl { display: flex; align-items: center; gap: 8px; flex: none; }
  @media (max-width: 1100px) { .devices { grid-template-columns: 1fr; } }
</style>
