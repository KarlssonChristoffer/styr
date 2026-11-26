<!-- The decision model in the status bar (Jev, TypeSafe's, from `apis.rs`): its logo, its name and what is left of the credit you entered. A click opens a card whose big number is that credit, counting down as calls land (today's spend until a credit is entered), a strip with one bar per decision and the latest few as one line each; the gear turns the card into what it may decide (the same switches as Settings > API connections) and the credit. Shown only while a connection that answers questions has its key.
     Used by StatusBar.svelte. -->
<script lang="ts">
  import { untrack } from "svelte";
  import { quiet } from "../quiet";
  import { poll } from "../poll";
  // The decision model in the status bar (Jev, TypeSafe's, from `apis.rs`): its logo, its name and
  // what is left of the credit you entered. A click opens a card whose big number is that credit,
  // counting down as calls land (today's spend until a credit is entered), a strip with one bar per
  // decision and the latest few as one line each; the gear turns the card into what it may decide
  // (the same switches as Settings > API connections) and the credit. Shown only while a connection
  // that answers questions has its key.
  import { Tween } from "svelte/motion";
  import { cubicInOut } from "svelte/easing";
  import { fade, slide } from "svelte/transition";
  import { api } from "../api";
  import { API_PRICE } from "../apiPrices";
  import { apis } from "../apis.svelte";
  import type { ApiDecision } from "../bindings/ApiDecision";
  import type { ApiState } from "../bindings/ApiState";
  import { hubdHas } from "../compat";
  import { clock, dollars } from "../format";
  import { BRANDS } from "../providerLogos";
  import { hub } from "../state.svelte";
  import { bar, onScreen } from "../statusbar.svelte";
  import ProviderLogo from "./ProviderLogo.svelte";
  import Select from "./Select.svelte";
  import Switch from "./Switch.svelte";

  let data = $state<ApiState | null>(null);
  let decisions = $state<ApiDecision[]>([]);
  let tab = $state<"thread" | "today">("thread");
  /** True for a moment after switching tabs, so the other tab's bars and rows appear at once
   *  instead of fading and sliding in, which made the card jump. */
  let calm = $state(false);
  function pick(t: "thread" | "today") {
    if (t === tab) return;
    calm = true;
    tab = t;
    setTimeout(() => (calm = false), 60);
  }
  let settings = $state(false);
  let fallbacks = $state(false);

  async function load() {
    try {
      const [s, d] = await Promise.all([api.apis(), api.apiDecisions()]);
      data = s;
      decisions = d;
    } catch {
      // An older or unreachable core: the item stays out.
    }
  }
  $effect(() => {
    // Every few seconds while the card is open, so the number moves as calls land.
    void load();
    const t = poll(load, bar.card === "jev" ? 5_000 : 60_000);
    return () => t();
  });
  $effect(() => {
    if (bar.card === "jev") void load();
    else settings = false;
  });

  /** The connection Hub's own decisions ask: the chosen one, else the first that answers and has its key. */
  const conn = $derived.by(() => {
    if (!data) return null;
    const asks = new Set(data.catalog.filter((k) => k.asks).map((k) => k.kind));
    const keyed = (name: string) => !!data?.status.find((s) => s.name === name)?.key;
    return data.prefs.connections.filter((c) => (data?.prefs.helper ?? c.name) === c.name).find((c) => asks.has(c.kind) && keyed(c.name)) ?? null;
  });
  const status = $derived(data?.status.find((s) => s.name === conn?.name) ?? null);
  const kind = $derived(data?.catalog.find((k) => k.kind === conn?.kind) ?? null);
  const health = $derived(!status?.check ? "unknown" : status.check.ok ? "ok" : "bad");

  const thread = $derived(bar.place.thread);
  const mine = $derived(decisions.filter((d) => d.connection === conn?.name));
  const here = $derived(thread ? mine.filter((d) => d.thread === thread) : []);
  const today = $derived.by(() => {
    const start = new Date();
    start.setHours(0, 0, 0, 0);
    return mine.filter((d) => d.at >= start.getTime());
  });
  const shown = $derived(tab === "thread" ? here : today);

  /** Dollars per million input tokens; output is free (docs.typesafe.ai/models). */
  /** The card shows only the latest few; the whole log is on the connection's settings page. */
  const SHOW = 4;
  const price = $derived(conn ? API_PRICE[conn.kind] ?? null : null);
  const money = (tokens: number) => (price === null ? null : dollars((tokens * price) / 1_000_000));
  const usd = (tokens: number) => (price === null ? null : (tokens * price) / 1_000_000);

  /** A newer core counts everything (`total`) and keeps the credit you entered. */
  type Credit = { usd: number; tokens_at: number; at: number };
  type Usage = ApiState["status"][number]["today"];
  const total = $derived((status as { total?: Usage | null } | null)?.total ?? null);
  const credit = $derived((conn as { credit?: Credit | null } | null)?.credit ?? null);
  const canCredit = $derived(!!status && "total" in status);
  /** What is left of the credit, in dollars. */
  const left = $derived.by(() => {
    if (!credit || !total || price === null) return null;
    return Math.max(0, credit.usd - ((total.input_tokens - credit.tokens_at) * price) / 1_000_000);
  });
  const spentToday = $derived(status ? usd(status.today.input_tokens) : null);
  /** Calls and time left at this month's pace. */
  const runway = $derived.by(() => {
    if (left === null || !status || price === null) return null;
    const m = status.month.calls ? status.month : status.today;
    const perCall = m.calls ? usd(m.input_tokens / m.calls) : null;
    const now = new Date();
    const days = now.getDate() - 1 + (now.getHours() * 60 + now.getMinutes()) / 1440;
    const perDay = days > 0.05 ? (usd(status.month.input_tokens) ?? 0) / days : 0;
    return { calls: perCall ? left / perCall : null, days: perDay ? left / perDay : null };
  });
  const lasts = (days: number) =>
    days < 60 ? `${Math.round(days)} days` : days < 730 ? `${Math.round(days / 30)} months` : days < 3650 ? `${(days / 365).toFixed(1)} years` : `${Math.round(days / 365)} years`;
  /** The big number, counting to each new value instead of jumping. */
  const big = $derived(left ?? spentToday ?? 0);
  const shownBig = new Tween(untrack(() => big), { duration: 1400, easing: cubicInOut });
  $effect(() => {
    shownBig.set(big);
  });
  /** "$3.98" and "7382": the digits that move are drawn apart. */
  const digits = (v: number) => {
    const t = v.toFixed(6);
    return [`$${t.slice(0, t.length - 4)}`, t.slice(-4)];
  };

  let creditText = $state("");
  function setCredit() {
    if (!data || !conn) return;
    const v = Number(creditText.replace(",", ".").replace("$", "").trim());
    if (!Number.isFinite(v) || v < 0) return;
    const connections = data.prefs.connections.map((c) => (c.name === conn?.name ? { ...c, credit: v > 0 ? { usd: v, tokens_at: 0, at: 0 } : null } : c));
    creditText = "";
    void save({ ...data.prefs, connections } as ApiState["prefs"]);
  }

  /** One decision as one line: a mark, what happened, the words, coloured by what it means. */
  function brief(d: ApiDecision): { mark: string; tone: string; title: string } {
    if (!d.acted) return { mark: "–", tone: "muted", title: "Unsure" };
    const s = d.said;
    const pick: Record<string, [string, string, string]> = {
      "turns:done": ["✓", "green", "Done"],
      "turns:asks": ["?", "amber", "Asks you"],
      "turns:stuck": ["!", "amber", "Stuck"],
      "turns:partway": ["◐", "", "Stopped part way"],
      "judge:ask": ["↳", "green", "Answered on the side"],
      "judge:stop": ["■", "amber", "Stopped it"],
      "judge:steer": ["↻", "brand", "Steered"],
      "judge:later": ["⋯", "", "Queued for later"],
      "judge:new": ["+", "brand", "Offered a new thread"],
      "topic:new": ["+", "brand", "Offered a new thread"],
      "topic:same": ["↻", "brand", "Same task"],
      "focus:none": ["!", "amber", "Off the stage"],
      "proof:failed": ["✕", "red", "Hidden failure"],
      "proof:passed": ["✓", "green", "Clean output"],
    };
    const [mark, tone, title] = pick[`${d.feature}:${s}`] ?? (d.feature === "focus" ? ["◎", "brand", "On the stage"] : ["•", "", s]);
    return { mark, tone, title };
  }
  /** The bars: the latest decisions, oldest first. */
  const SLOTS = 40;

  const short = (n: number) => (n >= 10_000 ? `${Math.round(n / 1000)}k` : n >= 1000 ? `${(n / 1000).toFixed(1)}k` : `${n}`);
  /** Tokens per decision arrive with a newer core; until then only the day's total is known. */
  const tokensOf = (list: ApiDecision[]) => {
    const known = list.map((d) => (d as ApiDecision & { tokens?: number | null }).tokens).filter((t): t is number => typeof t === "number");
    return known.length ? known.reduce((a, b) => a + b, 0) : null;
  };

  const NAMES: Record<string, string> = { turns: "Turn endings", judge: "While it works", topic: "Same task?", focus: "Focus", proof: "Test output", auto: "Model pick" };
  const WHAT: Record<string, string> = { turns: "done, asks you, stuck", judge: "ask, stop, steer, later, new", topic: "offers a new thread", focus: "which stage line", proof: "hidden failures" };
  const WITHOUT: Record<string, string> = {
    turns: "the thread just ends; only a permission ask counts as needs you",
    judge: "your own queue or steer setting decides",
    topic: "no offer of a thread of its own",
    focus: "the small model on your subscription decides",
    proof: "the exit code alone decides",
  };
  /** How sure each feature needs it to be before Hub goes by it (`reading.rs`, `judge.rs`). */
  const SURE: Record<string, number> = { turns: 0.6, focus: 0.6, proof: 0.75, topic: 0.75, judge: 0.6 };
  const sure = (f: string) => ((data?.prefs as { sure?: Record<string, number> } | undefined)?.sure?.[f] ?? SURE[f] ?? 0.6);

  /** What it read, in words. */
  function read(d: ApiDecision): string {
    if (d.feature === "turns") return "Turn ended · the agent said";
    if (d.feature === "judge") return "While it works · you wrote";
    if (d.feature === "topic") return "New message · you wrote";
    if (d.feature === "focus") return "New thread · you asked";
    if (d.feature === "proof") return "Test run · its output";
    return NAMES[d.feature] ?? d.feature;
  }

  /** The words it read, as short as they go. A newer core sends them as `quote`. */
  function quote(d: ApiDecision): string | null {
    const q = (d as ApiDecision & { quote?: string | null }).quote;
    if (q) return q;
    // An older core keeps only the start of what was read, which for these is the thread's first
    // message, not what you wrote: better none than the wrong words.
    if (d.feature === "judge" || d.feature === "topic") return null;
    const t = d.text.replace(/\s+/g, " ").trim();
    const reply = t.match(/Reply: (.*)/);
    if (reply) return reply[1];
    const command = t.match(/^Command: (.*?)( End of its output:|$)/);
    if (command) return command[1];
    return t.replace(/^(Request|The thread began with|New message): /, "");
  }

  /** What Hub did with it, and its tone. */
  function did(d: ApiDecision): { text: string; tone: string } {
    const pc = Math.round((d.confidence ?? 0) * 100);
    if (!d.acted) return { text: d.feature === "judge" ? "Not sure enough, your own setting decided" : "Not sure enough, nothing changed", tone: "muted" };
    const s = d.said;
    if (d.feature === "turns") {
      if (s === "done") return { text: "Marked done", tone: "" };
      if (s === "asks") return { text: "Moved to Needs you: it asks you something", tone: "amber" };
      if (s === "stuck") return { text: "Moved to Needs you: it is stuck", tone: "amber" };
      if (s === "partway") return { text: "Marked as stopped part way", tone: "" };
    }
    if (d.feature === "judge") {
      if (s === "ask") return { text: "Answered on the side, the agent kept working", tone: "green" };
      if (s === "stop") return (d.confidence ?? 0) >= 0.8 ? { text: "Stopped the agent", tone: "amber" } : { text: `Too unsure to stop (${pc}%), your setting decided`, tone: "muted" };
      if (s === "steer") return { text: "Steered the running work", tone: "" };
      if (s === "later") return { text: "Queued for after this turn", tone: "" };
      if (s === "new") return (d.confidence ?? 0) >= 0.75 ? { text: "Offered a thread of its own", tone: "" } : { text: `Too unsure it was new (${pc}%), your setting decided`, tone: "muted" };
    }
    if (d.feature === "topic") return s === "new" ? { text: "Offered a thread of its own", tone: "" } : { text: "Same task, stayed in this thread", tone: "" };
    if (d.feature === "focus") return s === "none" ? { text: "Marked off the stage", tone: "amber" } : { text: "Works towards a line of the stage", tone: "" };
    if (d.feature === "proof") return s === "failed" ? { text: "Caught a hidden failure; the agent was asked to look", tone: "red" } : { text: "Nothing hidden in it", tone: "" };
    return { text: s, tone: "" };
  }

  /** A core that keeps thresholds sends `sure`; an older one leaves them fixed. */
  const canSetSure = $derived(!!data && "sure" in data.prefs);
  const LEVELS = [0.5, 0.6, 0.7, 0.75, 0.8, 0.9, 0.95];

  /** A core that lets agents ask Jev (`apis.agent`, the `ask_jev` tool); older ones show it as coming. */
  let agentHere = $state(false);
  quiet(hubdHas("apis.agent").then((v) => (agentHere = v)), "hubd has"); // compat-until: 2026-12-06
  /** The global switch is the "agent" entry in the connection's off-list, like Hub's own features. */
  const agentsOn = $derived(!data?.prefs.off.includes("agent"));
  /** Whether agents in the open chat may ask: the core already combines both switches, so it is
   *  off here when all agents are off. Looked at again when the card opens or the chat changes. */
  let agentsHere = $state(true);
  $effect(() => {
    const id = thread;
    if (!agentHere || !id || bar.card !== "jev") return;
    quiet(api.threadJev(id).then((r) => (agentsHere = r.agents)), "thread jev");
  });
  async function setAgentsHere(on: boolean) {
    if (!thread) return;
    try {
      agentsHere = (await api.setThreadJev(thread, on)).agents;
    } catch (e) {
      hub.fail(e);
    }
  }

  function setFeature(f: string, on: boolean) {
    if (!data) return;
    void save({ ...data.prefs, off: on ? data.prefs.off.filter((x) => x !== f) : [...data.prefs.off, f] });
  }

  function setSure(f: string, v: number) {
    if (!data) return;
    const sure = { ...((data.prefs as { sure?: Record<string, number> }).sure ?? {}) };
    if (v === SURE[f]) delete sure[f];
    else sure[f] = v;
    void save({ ...data.prefs, sure } as ApiState["prefs"]);
  }

  async function save(prefs: ApiState["prefs"]) {
    if (!data || !conn) return;
    try {
      const saved = await api.setApis(prefs);
      data = { ...data, prefs: saved };
    } catch (e) {
      hub.fail(e);
    }
  }

  /** A core that tunes in the background (`apis.tune.run`). */
  const canTune = $derived(apis.tune !== null);
  $effect(() => {
    if (bar.card === "jev") quiet(apis.loadTune(), "load tune");
  });
  async function tune() {
    try {
      await apis.runTune();
    } catch (e) {
      hub.fail(e);
    }
  }

  function openPage() {
    bar.card = null;
    hub.apiOpen = conn?.name ?? null;
    hub.openSettings("apis");
  }

  /** Where the card's left edge goes, in the page's own (zoomed) pixels, so it stays on screen. */
  const at = (width: number) => {
    const z = Number(document.documentElement.style.zoom) || 1;
    return Math.max(6, Math.min(bar.cardX / z, innerWidth / z - width - 8));
  };
  const label = $derived(conn?.label || conn?.name || "");
  const brand = $derived(conn ? BRANDS[conn.kind] ?? "var(--accent)" : "var(--accent)");
</script>

{#if conn}
  <button class="it" class:on={bar.card === "jev"} data-tip="{label}: {left !== null ? `$${left.toFixed(4)} left · ` : ""}{here.length} decision{here.length === 1 ? '' : 's'} in this thread" onclick={(e) => bar.toggleCard("jev", e.currentTarget.getBoundingClientRect().left)}>
    <ProviderLogo id={conn.kind} label={label} size={12} />{label}
    {#if left !== null}<span class="dim">${left.toFixed(2)}</span>{:else if thread && here.length}<span class="dim">{here.length}</span>{/if}
    {#if health === "bad"}<span class="bad-dot"></span>{/if}
  </button>
{/if}

{#if conn && bar.card === "jev"}
  <div class="bar-card jev" style:left="{at(380)}px" use:onScreen={bar.cardX} style:--brand={brand}>
    <div class="hd">
      <span class="wash"></span>
      <ProviderLogo id={conn.kind} label={label} size={15} />
      <b>{label}</b>
      <span class="state {health}">● {health === "ok" ? "Works" : health === "bad" ? "Failing" : "Not checked"}</span>
      <span class="who">{conn.kind === "typesafe" ? "TypeSafe" : kind?.label ?? ""}</span>
      <button class="gear" class:on={settings} aria-label="What {label} may decide" data-tip="What {label} may decide" onclick={() => (settings = !settings)}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="12" cy="12" r="3"></circle><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"></path></svg>
      </button>
    </div>

    {#snippet mainView()}
      <div class="tabs">
        {#if thread}<button class="tab" class:on={tab === "thread"} onclick={() => pick("thread")}>This thread</button>{/if}
        <button class="tab" class:on={tab === "today" || (!thread && tab === "thread")} onclick={() => pick("today")}>Today</button>
      </div>
      {@const list = tab === "thread" && !thread ? today : shown}
      {@const isToday = tab === "today" || (tab === "thread" && !thread)}
      {@const hereSpend = isToday ? spentToday : (() => { const t = tokensOf(list); return t === null ? null : usd(t); })()}
      {@const [head, tail] = digits(shownBig.current)}
      {@const bars = list.slice(0, SLOTS).reverse()}
      <div class="tick">
        <div class="big" data-tip={left !== null ? `What is left of the $${credit?.usd} you entered, counted by Hub` : "What Jev cost today"}>{head}<span class="moving">{tail}</span></div>
        <div class="cap">
          {#if left !== null}left of ${credit?.usd.toFixed(2)}{:else}spent today{/if}
        </div>
        <span class="live" class:bad={health === "bad"}>{health === "bad" ? "failing" : "live"}</span>
      </div>
      <div class="facts">
        {#if left !== null}
          <span>{isToday ? "today" : "this thread"} <b>{hereSpend !== null ? `$${hereSpend.toFixed(6)}` : "–"}</b></span>
          {#if runway?.calls}<span>≈ <b>{short(Math.round(runway.calls))}</b> calls left</span>{/if}
          {#if runway?.days}<span>~<b>{lasts(runway.days)}</b> left</span>{/if}
        {:else}
          <span><b>{isToday && status ? status.today.calls : list.length}</b> calls</span>
          {#if isToday && status}<span><b>{short(status.today.input_tokens + status.today.output_tokens)}</b> tokens</span>{/if}
          {#if !isToday && hereSpend !== null}<span>this thread <b>${hereSpend.toFixed(6)}</b></span>{/if}
        {/if}
        {#if isToday && status?.today.errors}<span class="red"><b>{status.today.errors}</b> errors</span>{/if}
      </div>
      <div class="strip" data-tip="One bar per decision; colour is what Hub did, height how sure it was">
        {#each bars as d (`${d.at}:${d.feature}:${d.thread}`)}
          <i class={brief(d).tone} style:height="{Math.max(4, Math.round((d.confidence ?? 0) * 30))}px" in:fade={{ duration: calm ? 0 : 160 }}></i>
        {/each}
        {#each { length: SLOTS - bars.length } as _, i (i)}<i class="slot"></i>{/each}
      </div>
      <div class="axis"><span>{bars.length ? clock(bars[0].at) : ""}</span><span>now</span></div>
      <div class="list">
        {#each list.slice(0, SHOW) as d (`${d.at}:${d.feature}:${d.thread}`)}
          {@const b = brief(d)}
          {@const q = quote(d)}
          {@const need = sure(d.feature)}
          {@const pc = Math.round((d.confidence ?? 0) * 100)}
          <div class="ev" data-tip="{read(d)} → {did(d).text}" in:slide={{ duration: calm ? 0 : 200 }} out:fade={{ duration: calm ? 0 : 160 }}>
            <span class="mk {b.tone}">{b.mark}</span>
            <span class="tx">{b.title}<span>{q ? ` · "${q}"` : !d.acted ? (d.feature === "judge" ? " · your setting decided" : " · nothing changed") : ""}</span></span>
            <svg class="ring {b.tone}" viewBox="0 0 20 20" aria-hidden="true" data-tip="{pc}% sure; Hub goes by it from {Math.round(need * 100)}%"><circle cx="10" cy="10" r="7.5" class="bg"></circle><circle cx="10" cy="10" r="7.5" class="fg" stroke-dasharray="{(pc / 100) * 47.1} 47.1" transform="rotate(-90 10 10)"></circle></svg>
            <span class="pc">{pc}%</span>
            <span class="tm">{clock(d.at)}</span>
          </div>
        {:else}
          <div class="ev"><span class="mk ghost"></span><span class="tx quiet">{tab === "thread" && thread ? `${label} has not decided anything in this thread yet.` : "Nothing decided today."}</span></div>
        {/each}
        <!-- Empty rows keep the card one height on both tabs, so switching does not make it jump. -->
        {#each { length: SHOW - Math.max(1, Math.min(list.length, SHOW)) } as _, i (i)}<div class="ev blank" aria-hidden="true"><span class="mk ghost"></span><span class="tx">&nbsp;</span></div>{/each}
      </div>
      <!-- One row on both tabs, empty on Today, so the card keeps its height when switching. -->
      {#if tab === "thread" && thread}
        <div class="row agent">
          <span class="nm" data-tip="Whether agents in this chat may ask {label} a question. Off never stops the agent, it decides itself.">Agents can ask {label} here{#if agentHere && !agentsOn}<span>off for all agents</span>{/if}</span>
          {#if agentHere}
            <Switch label="Agents can ask {label} in this chat" on={agentsHere} disabled={!agentsOn} onchange={setAgentsHere} />
          {:else}
            <span class="need">comes with the next version</span>
          {/if}
        </div>
      {:else}
        <div class="row agent blank" aria-hidden="true"><span class="nm">&nbsp;</span></div>
      {/if}
      <div class="foot">
        <span>{list.length > SHOW ? `${list.length - SHOW} more ${isToday ? "today" : "here"}` : ""}</span>
        <span class="acts">
          {#if canTune}
            {#if apis.tune?.running}<span class="tuning" data-tip="Reading {label}'s decisions in the background">Tuning…</span>
            {:else if apis.open.length}<button class="chip" onclick={openPage} data-tip="What the last tune suggests, on {label}'s page">{apis.open.length} suggestion{apis.open.length === 1 ? "" : "s"} ›</button>
            {:else}<button class="link" onclick={tune} data-tip="Reads {label}'s decisions, what it read and what happened after, in the background, and suggests how to make it better">Tune ›</button>{/if}
          {/if}
          <button class="link accent" onclick={openPage}>Full log ›</button>
        </span>
      </div>
    {/snippet}
    {#snippet gearView()}
      <div class="row head"><button class="back" aria-label="Back" onclick={() => (settings = false)}>‹</button><span class="nm strong">What {label} may decide</span><span class="dim">acts at</span></div>
      {#each (data?.features ?? []).filter((f) => f.name !== "agent") as f (f.name)}
        <div class="row">
          <span class="nm" data-tip={f.what}>{NAMES[f.name] ?? f.name}<span>{WHAT[f.name] ?? ""}</span></span>
          {#if canSetSure}
            <Select size="small" label="How sure {NAMES[f.name] ?? f.name} must be" value={sure(f.name)} onchange={(v) => setSure(f.name, v)}
              options={[...new Set([...LEVELS, sure(f.name)])].sort().map((v) => ({ value: v, label: `${Math.round(v * 100)}%` }))} />
            {#if f.name === "judge"}<span class="need" data-tip="Stopping the agent needs 80%, offering a new thread 75%">· stop 80%</span>{/if}
          {:else}
            <span class="need">{Math.round(sure(f.name) * 100)}%{f.name === "judge" ? " · stop 80%" : ""}</span>
          {/if}
          <Switch label={NAMES[f.name] ?? f.name} on={!data?.prefs.off.includes(f.name)} onchange={(v) => setFeature(f.name, v)} />
        </div>
      {/each}
      <div class="row">
        <span class="nm" data-tip="Agents ask {label} through Hub's ask_jev tool. Off: it answers at once &quot;decide yourself&quot;, nothing is called and nothing costs. A chat can differ on its own tab.">Agents can ask {label}<span>branch points while they work</span></span>
        {#if agentHere}
          <Switch label="Agents can ask {label}" on={agentsOn} onchange={(v) => setFeature("agent", v)} />
        {:else}
          <span class="need">comes with the next version</span>
        {/if}
      </div>
      <div class="sub"><span>Connection</span><span></span></div>
      <div class="inline">
        {#if conn.model}<span>Model <b>{conn.model}</b></span>{/if}
        <span>Daily cap <b>{conn.daily_tokens ? `${short(conn.daily_tokens)} tokens` : "none"}</b></span>
        {#if price !== null}<span>Price <b>${price} / M tokens in</b></span>{/if}
      </div>
      <div class="row credit">
        <span class="nm" data-tip="{kind?.label ?? label} can't tell Hub your balance, so enter what you put on the account; Hub counts down from it. Enter the new amount when you top up.">Credit on the account<span>{credit ? `$${credit.usd.toFixed(2)} entered ${new Date(credit.at).toLocaleDateString()}` : "not entered"}</span></span>
        {#if canCredit}
          <input class="field" inputmode="decimal" placeholder={credit ? `$${credit.usd}` : "$4.00"} bind:value={creditText} onkeydown={(e) => e.key === "Enter" && setCredit()} />
          <button class="link accent" onclick={setCredit} disabled={!creditText.trim()}>Save</button>
        {:else}
          <span class="need">needs the next Hub version</span>
        {/if}
      </div>
      {#if fallbacks}
        <div class="sub"><span>Without {label}</span><span></span></div>
        {#each (data?.features ?? []).filter((f) => f.name !== "agent") as f (f.name)}
          <div class="fb"><b>{NAMES[f.name] ?? f.name}</b> {WITHOUT[f.name] ?? "Hub does what it did before"}</div>
        {/each}
      {/if}
      <div class="foot">
        <button class="link" onclick={() => (fallbacks = !fallbacks)}>Without {label}, Hub uses its own rules {fallbacks ? "‹" : "›"}</button>
        <button class="link accent" onclick={openPage}>Key ›</button>
      </div>
    {/snippet}
    <!-- Both views are always laid out, one hidden, so the card is as tall as the taller one and
         flipping to the gear and back never changes its height. -->
    <div class="views">
      <div class="view" class:off={settings} inert={settings}>{@render mainView()}</div>
      <div class="view" class:off={!settings} inert={!settings}>{@render gearView()}</div>
    </div>
  </div>
{/if}

<style>
  .it { display: inline-flex; align-items: center; gap: 4px; height: calc(var(--bar-h) - 4px); padding: 0 8px; border-radius: var(--r-xs); white-space: nowrap; color: var(--muted); font-size: 12px; }
  .it:hover, .it.on { background: var(--surface-hover); color: var(--text-2); }
  .dim { color: var(--muted); }
  .bad-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--red); }

  .jev {
    position: fixed; bottom: calc(var(--bar-h) + 6px); z-index: 60; width: 380px; max-height: calc(100vh - var(--bar-h) - 24px); display: flex; flex-direction: column;
    background: var(--surface); border: 1px solid var(--line-strong); border-radius: var(--r-md); overflow: hidden;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.45); font-size: 13px; color: var(--text-2);
  }
  .views { display: grid; grid-template-rows: minmax(0, 1fr); min-height: 0; flex: 1 1 auto; }
  .view { grid-area: 1 / 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .view.off { visibility: hidden; pointer-events: none; }
  .view > .foot { margin-top: auto; }
  /* TypeSafe's clouds, faint and dithered, behind the head only. */
  .hd { position: relative; display: flex; align-items: center; gap: 8px; padding: 10px 12px 8px; flex: none; }
  .hd > :not(.wash) { position: relative; }
  .wash {
    position: absolute; inset: 0; opacity: 0.24; pointer-events: none;
    background: radial-gradient(ellipse 40% 120% at 0% 50%, #e650bb, transparent 70%), radial-gradient(ellipse 40% 120% at 100% 30%, var(--brand), transparent 70%), linear-gradient(90deg, transparent, #8fc3dc66);
  }
  .wash::after { content: ""; position: absolute; inset: 0; background-image: radial-gradient(rgba(0, 0, 0, 0.5) 0.8px, transparent 1.1px); background-size: 3px 3px; }
  .hd b { color: var(--text); font-weight: 600; font-size: 13px; }
  .state { font-size: 12px; color: var(--muted); }
  .state.ok { color: var(--green); }
  .state.bad { color: var(--red); }
  .who { margin-left: auto; color: var(--muted); font-size: 12px; }
  .gear { width: 24px; height: 24px; border-radius: var(--r-sm); display: grid; place-items: center; color: var(--muted); }
  .gear:hover, .gear.on { background: rgba(0, 0, 0, 0.25); color: var(--text-2); }

  .tabs { display: flex; gap: 2px; padding: 8px 8px 6px; flex: none; }
  .tab { padding: 3px 8px; border-radius: var(--r-sm); color: var(--muted); font-size: 12px; }
  .tab:hover { color: var(--text-2); }
  .tab.on { background: var(--surface-hover); color: var(--text); }
  .tick { display: flex; align-items: center; gap: 12px; padding: 10px 12px 6px; flex: none; }
  .big { font-size: 28px; font-weight: 500; line-height: 1; color: var(--text); font-variant-numeric: tabular-nums; letter-spacing: -0.02em; }
  .moving { color: color-mix(in srgb, #e650bb 45%, var(--text)); }
  .cap { color: var(--muted); font-size: 12px; line-height: 1.35; white-space: nowrap; }
  .live { margin-left: auto; align-self: center; display: inline-flex; align-items: center; gap: 4px; color: color-mix(in srgb, var(--brand) 70%, var(--text)); font-size: 11px; }
  .live::before { content: ""; width: 6px; height: 6px; border-radius: 50%; background: var(--brand); box-shadow: 0 0 0 3px color-mix(in srgb, var(--brand) 20%, transparent); }
  .live.bad { color: var(--red); }
  .live.bad::before { background: var(--red); box-shadow: none; }
  .facts { display: flex; flex-wrap: wrap; gap: 2px 14px; padding: 2px 12px 10px; color: var(--muted); font-size: 12px; flex: none; }
  .facts b { color: var(--text-2); font-weight: 500; font-variant-numeric: tabular-nums; }
  .facts .red, .facts .red b { color: var(--red); }
  .strip { display: flex; align-items: flex-end; gap: 3px; height: 32px; padding: 0 12px; flex: none; }
  .strip i { flex: 1; min-width: 2px; border-radius: 2px; background: var(--line-strong); }
  .strip i.green { background: var(--green); }
  .strip i.amber { background: var(--amber); }
  .strip i.red { background: var(--red); }
  .strip i.brand { background: var(--brand); }
  .strip i.muted { background: var(--line-strong); }
  .strip i.slot { height: 2px; background: var(--line); }
  .axis { display: flex; justify-content: space-between; padding: 4px 12px 8px; color: var(--muted); font-size: 11px; flex: none; }
  .sub { display: flex; justify-content: space-between; padding: 8px 12px 2px; font-size: 11px; color: var(--muted); border-top: 1px solid var(--line); flex: none; }
  .list { overflow: auto; min-height: 0; border-top: 1px solid var(--line); }

  .ev { display: flex; align-items: center; gap: 8px; padding: 6px 12px; border-top: 1px solid var(--line); }
  .ev:first-child { border-top: 0; }
  .mk { width: 20px; height: 20px; border-radius: var(--r-sm); display: grid; place-items: center; flex: none; font-size: 11px; background: var(--surface-hover); color: var(--text-3); }
  .mk.green { color: var(--green); background: color-mix(in srgb, var(--green) 13%, transparent); }
  .mk.amber { color: var(--amber); background: color-mix(in srgb, var(--amber) 14%, transparent); }
  .mk.red { color: var(--red); background: color-mix(in srgb, var(--red) 14%, transparent); }
  .mk.brand { color: color-mix(in srgb, var(--brand) 75%, var(--text)); background: color-mix(in srgb, var(--brand) 14%, transparent); }
  .mk.muted { color: var(--muted); }
  .mk.ghost { background: none; }
  .ev.blank { border-top-color: transparent; }
  .tx.quiet { color: var(--muted); }
  .tx { flex: 1; min-width: 0; color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tx span { color: var(--muted); }
  .ring { width: 16px; height: 16px; flex: none; }
  .ring .bg { fill: none; stroke: var(--line-strong); stroke-width: 2.5; }
  .ring .fg { fill: none; stroke: var(--text-3); stroke-width: 2.5; stroke-linecap: round; }
  .ring.green .fg { stroke: var(--green); }
  .ring.amber .fg { stroke: var(--amber); }
  .ring.red .fg { stroke: var(--red); }
  .ring.brand .fg { stroke: var(--brand); }
  .ring.muted .fg { stroke: var(--muted); }
  .pc { width: 28px; text-align: right; color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; }
  .tm { min-width: 36px; text-align: right; white-space: nowrap; color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; }
  .quiet { margin: 10px 12px; color: var(--muted); }

  .row { display: flex; align-items: center; gap: 10px; padding: 6px 12px; min-height: 30px; }
  .row.head { padding-top: 8px; }
  .back { color: var(--muted); font-size: 14px; padding: 0 2px; }
  .back:hover { color: var(--text); }
  .nm { flex: 1; min-width: 0; color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .nm.strong { color: var(--text); font-weight: 500; }
  .nm span { color: var(--muted); font-size: 12px; margin-left: 6px; }
  .need { color: var(--text-3); font-size: 12px; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .jev :global(.switch.on) { background: color-mix(in srgb, var(--brand) 18%, transparent); }
  .jev :global(.switch.on span) { background: var(--brand); }
  .inline { display: flex; flex-wrap: wrap; gap: 4px 14px; padding: 6px 12px 8px; color: var(--muted); font-size: 12px; }
  .inline b { color: var(--text-2); font-weight: 400; }
  .credit .field { width: 76px; height: 24px; padding: 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); font-size: 12px; outline: none; color: var(--text); }
  .credit .field:focus { border-color: var(--brand); }
  .fb { padding: 3px 12px; color: var(--muted); font-size: 12px; }
  .fb b { color: var(--text-2); font-weight: 500; }

  .foot { display: flex; justify-content: space-between; gap: 10px; padding: 8px 12px; border-top: 1px solid var(--line); color: var(--muted); font-size: 12px; flex: none; }
  .link { font-size: 12px; color: var(--muted); padding: 0; }
  .acts { display: flex; gap: 14px; }
  .link:hover { color: var(--text-2); }
  .link.accent { color: var(--accent); }
  .chip { font-size: 12px; padding: 0 8px; border-radius: var(--r-md); color: color-mix(in srgb, var(--brand) 70%, var(--text)); background: color-mix(in srgb, var(--brand) 13%, transparent); }
  .chip:hover { background: color-mix(in srgb, var(--brand) 20%, transparent); }
  .tuning { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; }
  .tuning::before { content: ""; width: 6px; height: 6px; border-radius: 50%; background: var(--brand); animation: breathe 2.4s var(--ease-move) infinite; }
  @keyframes breathe { 0%, 100% { opacity: 0.35; } 50% { opacity: 1; } }
</style>
