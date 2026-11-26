<!-- Settings > Where it goes: all AI use of the last days, in Hub and outside it. One total, then a card per place it happened (Hub, Claude in all its places, Codex on its own), each with its own week, a flat tint of its colour and the share it had. Under them, Hub's part by who asked and by engine (numbers and chips under one slim split strip, no percent columns), and what was paid for by the token (DeepSeek, Jev's connection). Last, what Hub sent to the AIs, read from its packing slips (`package.ledger`), in tokens, as lines under the same kind of strip. A hubd with `usage.spend` counts Claude, Codex and DeepSeek per day; an older one is read through /api/stats, which knows Claude only and no days. While a thread works the numbers are read again every few seconds, so a running thread shows as it spends. A hubd with `usage.spend.machines` adds every machine's count together and names each one's part; with `usage.spend.cloud` it has a lane for Claude Code sessions in Anthropic's cloud; with `usage.spend.accounts` Codex's own count of the whole account stands on its card.
     Used by Settings.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  // Settings > Where it goes: all AI use of the last days, in Hub and outside it. One total, then a
  // card per place it happened (Hub, Claude in all its places, Codex on its own), each with its own
  // week, a flat tint of its colour and the share it had. Under them, Hub's part by who asked and
  // by engine (numbers and chips under one slim split strip, no percent columns), and what was paid
  // for by the token (DeepSeek, Jev's connection). Last, what Hub sent to the AIs, read from its
  // packing slips (`package.ledger`), in tokens, as lines under the same kind of strip.
  // A hubd with `usage.spend` counts Claude, Codex and DeepSeek per day; an older one is read
  // through /api/stats, which knows Claude only and no days. While a thread works the numbers are
  // read again every few seconds, so a running thread shows as it spends.
  // A hubd with `usage.spend.machines` adds every machine's count together and names each one's
  // part; with `usage.spend.cloud` it has a lane for Claude Code sessions in Anthropic's cloud;
  // with `usage.spend.accounts` Codex's own count of the whole account stands on its card.
  import { api } from "../api";
  import { API_PRICE } from "../apiPrices";
  import { hubdHas, tooOld } from "../compat";
  import { shownPlan } from "../plan";
  import { BRANDS } from "../providerLogos";
  import { hub } from "../state.svelte";
  import type { ApiState } from "../bindings/ApiState";
  import type { PackageSummary } from "../bindings/PackageSummary";
  import type { Spend } from "../bindings/Spend";
  import type { SpendEngine } from "../bindings/SpendEngine";
  import type { Stats } from "../bindings/Stats";
  import Icon from "./Icon.svelte";
  import ProviderLogo from "./ProviderLogo.svelte";
  import ProviderMark from "./ProviderMark.svelte";
  import WhereZoom from "./WhereZoom.svelte";

  type Lane = { key: string; cost: number; calls: number | null; tokens: number | null; daily: number[] | null; engines: SpendEngine[]; priced: boolean; parts?: Lane[] };

  /** What each lane is called and how it looks. A lane this window does not know is drawn plainly. */
  const LANES: Record<string, { name: string; sub: string; mark: "hub" | "anthropic" | "openai"; engine?: string }> = {
    hub: { name: "Hub", sub: "everything run through Hub", mark: "hub" },
    claude_code: { name: "Claude Code", sub: "on its own, the terminal and the Claude app", mark: "anthropic", engine: "claude" },
    claude_cloud: { name: "In the cloud", sub: "project threads, claude.ai/code and Cowork", mark: "anthropic" },
    codex: { name: "Codex", sub: "on its own, the CLI and ChatGPT", mark: "openai", engine: "codex" },
    claude_elsewhere: { name: "Elsewhere", sub: "claude.ai chat and more, from the plan meter", mark: "anthropic" },
    claude: { name: "Claude", sub: "the terminal, the cloud and claude.ai", mark: "anthropic", engine: "claude" },
  };
  const ORDER = ["hub", "claude_code", "claude_cloud", "claude_elsewhere", "codex"];
  /** Claude's three lanes share one card, each a part of its bars in its own colour. The estimate
   *  is striped and never sets the height of a chart: a bar it makes taller than the rest is cut. */
  const CLAUDE: Record<string, { name: string; rgb: string; estimate?: boolean }> = {
    claude_code: { name: "Terminal", rgb: "217, 119, 87" },
    claude_cloud: { name: "In the cloud", rgb: "120, 160, 220" },
    claude_elsewhere: { name: "Elsewhere", rgb: "204, 160, 112", estimate: true },
  };
  /** The lanes as one Claude lane, put where the first of them stood. */
  function together(list: Lane[]): Lane[] {
    const parts = list.filter((l) => CLAUDE[l.key]);
    if (!parts.length) return list;
    const at = list.indexOf(parts[0]);
    const daily = parts.every((l) => l.daily) ? parts[0].daily!.map((_, i) => parts.reduce((n, l) => n + (l.daily![i] ?? 0), 0)) : null;
    const sum = (f: (l: Lane) => number | null) => (parts.some((l) => f(l) === null) ? null : parts.reduce((n, l) => n + (f(l) ?? 0), 0));
    const claude: Lane = {
      key: "claude",
      cost: parts.reduce((n, l) => n + l.cost, 0),
      calls: parts.find((l) => l.key === "claude_code")?.calls ?? null,
      tokens: sum((l) => (CLAUDE[l.key].estimate ? 0 : l.tokens)),
      daily,
      engines: [],
      priced: parts.every((l) => l.priced),
      parts,
    };
    return [...list.slice(0, at).filter((l) => !CLAUDE[l.key]), claude, ...list.slice(at).filter((l) => !CLAUDE[l.key])];
  }
  /** A day's dollars without the estimate: what sets the height of a chart. */
  const measured = (l: Lane, i: number) => (l.daily?.[i] ?? 0) - (l.parts ?? []).filter((p) => CLAUDE[p.key]?.estimate).reduce((n, p) => n + (p.daily?.[i] ?? 0), 0);
  /** Shown only when the hubd counts them. */
  const OPTIONAL = new Set(["claude_cloud", "claude_elsewhere"]);
  /** Lanes that are an estimate, with nothing of their own to count. */
  const ESTIMATED = new Set(["claude_elsewhere"]);
  /** Hub's engines in the bars, with the colour each is drawn in. */
  const ENGINES: Record<string, { name: string; mark: "anthropic" | "openai" | "deepseek"; colour: string; rgb: string }> = {
    claude: { name: "Claude", mark: "anthropic", colour: "#d97757", rgb: "217, 119, 87" },
    codex: { name: "Codex", mark: "openai", colour: "#9b8cff", rgb: "155, 140, 255" },
    deepseek: { name: "DeepSeek", mark: "deepseek", colour: "#4d6bfe", rgb: "77, 107, 254" },
  };
  /** Who asked, for the "Inside Hub" split: these are yours, the rest is Hub on its own. */
  const YOURS = new Set(["you", "side_job", "chat"]);

  /** "All time" in `days`: the hubd counts from the first day it has anything for. */
  const ALL = 0;
  const RANGES = [
    { days: 1, label: "Today" },
    { days: 7, label: "7 days" },
    { days: 30, label: "30 days" },
    { days: ALL, label: "All time" },
  ];
  let days = $state(7);
  /** What the page counts in: dollars (what the same tokens would cost through the API) or the
   *  tokens themselves, from a hubd with `usage.spend.tokens`. Remembered on this computer. */
  const UNIT_KEY = "hub.whereItGoes.unit";
  let unit = $state<"dollars" | "tokens">("dollars");
  try {
    if (localStorage.getItem(UNIT_KEY) === "tokens") unit = "tokens";
  } catch {
    // Dollars, then.
  }
  let canTokens = $state(false);
  hubdHas("usage.spend.tokens").then((has) => (canTokens = has), () => {}); // compat-until: 2026-12-05
  /** Tokens only when the hubd counts them; an older one keeps dollars whatever was chosen. */
  const inTokens = $derived(unit === "tokens" && canTokens);
  function pickUnit(u: "dollars" | "tokens") {
    unit = u;
    try {
      localStorage.setItem(UNIT_KEY, u);
    } catch {
      // Kept for this visit only.
    }
  }
  /** Whether the hubd can count from the first day it has (`usage.spend.all`). */
  let canAll = $state(false);
  hubdHas("usage.spend.all").then((has) => (canAll = has), () => {}); // compat-until: 2026-12-03
  const tz = -new Date().getTimezoneOffset();

  let spend = $state<Spend | null>(null);
  /** What one bar stands for: a long range is drawn in weeks or months, so the bars stay readable. */
  let per = $state<"day" | "week" | "month">("day");
  /** The days of a long range added into weeks (up to about a year) or 30-day blocks, counted back
   *  from today so the last bar ends today. Each block is dated by its first day. */
  function coarse(s: Spend): Spend {
    const n = s.dates.length;
    const size = n <= 62 ? 1 : n <= 434 ? 7 : 30;
    per = size === 1 ? "day" : size === 7 ? "week" : "month";
    if (size === 1) return s;
    const starts: number[] = [];
    for (let end = n; end > 0; end -= size) starts.unshift(Math.max(0, end - size));
    const sum = (daily: number[]) => starts.map((a, i) => daily.slice(a, i + 1 < starts.length ? starts[i + 1] : n).reduce((x, y) => x + y, 0));
    return {
      ...s,
      dates: starts.map((a) => s.dates[a]),
      lanes: s.lanes.map((l) => ({ ...l, daily: sum(l.daily), engines: l.engines.map((e) => ({ ...e, daily: sum(e.daily) })) })),
      accounts: s.accounts?.map((a) => ({ ...a, daily: sum(a.daily) })),
    };
  }
  let stats = $state<Stats | null>(null);
  /** The unit of the numbers on screen: the one they were read in, not the one just picked, so the
   *  old numbers never show in the new unit for the moment until the new ones arrive. */
  const shownTokens = $derived(spend ? spend.unit === "tokens" : inTokens);
  let apis = $state<ApiState | null>(null);
  let full = $state<boolean | null>(null);
  let failed = $state<string | null>(null);

  async function load(asked: number, tokens = inTokens) {
    const has = await quiet(hubdHas("usage.spend"), "hubd has", false); // compat-until: 2026-12-03
    if (has) {
      try {
        const everywhere = await quiet(hubdHas("usage.spend.machines"), "hubd has", false); // compat-until: 2026-12-03
        const s = asked === ALL ? await api.spend(90, tz, everywhere, true, false, tokens) : await api.spend(asked, tz, everywhere, false, false, tokens);
        if (asked === days && tokens === inTokens) [spend, full, failed] = [coarse(s), true, null];
        return;
      } catch (e) {
        if (!tooOld(e)) return void (failed = String(e));
      }
    }
    try {
      const s = await api.stats(asked, tz);
      if (asked === days) [stats, full, failed] = [s, false, null];
    } catch (e) {
      failed = String(e);
    }
  }
  let sent = $state<PackageSummary | null>(null);
  /** Every day there is and, from a hubd with `usage.spend.hours`, the last 31 days' hours: what
   *  the opened chart zooms over, whatever the range. Read when a card opens and while threads run. */
  let zoomData = $state<Spend | null>(null);
  async function loadZoom() {
    if (!full) return;
    try {
      const [everywhere, all, hours] = await Promise.all(["usage.spend.machines", "usage.spend.all", "usage.spend.hours"].map((f) => quiet(hubdHas(f), "hubd has", false))); // compat-until: 2026-12-04
      const tokens = inTokens;
      const z = await api.spend(all ? 1 : 90, tz, everywhere, all, hours, tokens);
      if (tokens === inTokens) zoomData = z;
    } catch {
      // The chart keeps the range's own days.
    }
  }

  async function loadSent(asked: number) {
    if (!(await quiet(hubdHas("package.ledger"), "hubd has", false))) return; // compat-until: 2026-12-03
    try {
      const s = await api.packageSummary(asked === ALL ? 3650 : asked, tz);
      if (asked === days) sent = s;
    } catch {
      // The section stays away; the rest of the page does not need it.
    }
  }
  $effect(() => {
    void load(days, inTokens);
    void loadSent(days);
  });
  // Another unit reads the opened chart's days again.
  $effect(() => {
    void inTokens;
    zoomData = null;
  });
  api.apis().then((a) => (apis = a), () => {});

  /** Threads working now; while there are any the numbers are read again. */
  const running = $derived(hub.threads.filter((t) => t.status === "running").length);
  $effect(() => {
    if (!running) return;
    const asked = days;
    const timer = poll(() => {
      void load(asked);
      void loadSent(asked);
      if (open) void loadZoom();
    }, 15_000);
    return () => timer();
  });

  const lanes = $derived.by((): Lane[] => together(apart()));
  function apart(): Lane[] {
    if (full && spend) {
      const by = new Map(spend.lanes.map((l) => [l.key, l]));
      const keys = [...ORDER.filter((k) => by.has(k) || !OPTIONAL.has(k)), ...spend.lanes.map((l) => l.key).filter((k) => !ORDER.includes(k))];
      return keys.map((k) => {
        const l = by.get(k);
        return { key: k, cost: l?.cost ?? 0, calls: l?.calls ?? 0, tokens: l?.tokens ?? 0, daily: l?.daily ?? spend!.dates.map(() => 0), engines: l?.engines ?? [], priced: l?.priced ?? true };
      });
    }
    if (!stats) return [];
    // An older hubd: Hub's threads from /api/stats, the rest of Claude's total ran on its own. compat-until: 2026-12-03
    const inHub = stats.threads.reduce((n, t) => n + t.cost, 0);
    return [
      { key: "hub", cost: inHub, calls: null, tokens: null, daily: null, engines: [{ key: "claude", cost: inHub, daily: [], paid: false }], priced: true },
      { key: "claude_code", cost: Math.max(0, stats.cost - inHub), calls: null, tokens: null, daily: null, engines: [], priced: true },
      { key: "codex", cost: 0, calls: null, tokens: null, daily: null, engines: [], priced: true },
    ];
  }
  /** The card opened for its details, or none. */
  let open = $state<string | null>(null);
  /** Hub's view names its biggest sources and folds the rest into one row, unless opened. */
  const TOP_SOURCES = 4;
  let everySource = $state(false);
  const opened = $derived(lanes.find((l) => l.key === open && l.daily) ?? null);
  $effect(() => {
    if (opened && full && !zoomData) void loadZoom();
  });
  /** Who asked, by name, for the opened Hub card. */
  const SOURCES: Record<string, string> = {
    you: "Your threads", side_job: "Side jobs", chat: "Chats", vakten: "Guard", routine: "Routines", merge: "Merge queue",
    widget: "Widgets", fixer: "Fix threads", customs: "Customs", agent: "Threads a thread started", helper: "Helper calls", deleted: "Threads since removed",
  };
  const day = (d: string | undefined) =>
    d
      ? new Date(`${d}T12:00:00`).toLocaleDateString(
          undefined,
          per !== "day" ? { day: "numeric", month: "short" } : days > 7 || days === ALL ? { day: "numeric" } : { weekday: "short" },
        )
      : "";
  const fmtTokens = (n: number) => (n >= 1_000_000 ? `${(n / 1_000_000).toFixed(1)} M` : n >= 1000 ? `${Math.round(n / 1000)} k` : `${n}`);
  const hubLane = $derived(lanes.find((l) => l.key === "hub") ?? null);
  /** Each machine's part, when more than this one was asked. */
  const machines = $derived(full && spend?.machines && spend.machines.length > 1 ? spend.machines : null);
  /** A provider's own count of its whole account, by lane. */
  const account = (key: string) => (full ? (spend?.accounts?.find((a) => a.key === key) ?? null) : null);
  /** A lane counted as running totals has no answers to count. */
  const answers = (l: Lane) => (l.key === "claude_cloud" || ESTIMATED.has(l.key) ? null : l.calls);
  /** Why a figure has a ≈ in front of it. */
  const roughly = (l: Lane) => (ESTIMATED.has(l.key) ? "Estimated: what Claude's weekly plan meter climbed beyond what Hub counted" : "A model without a known price is counted at a similar one's");
  const total = $derived(lanes.reduce((n, l) => n + l.cost, 0));
  /** One scale for every card, so a tall bar means the same in each. */
  const top = $derived.by(() => {
    const counted = Math.max(0.0001, ...lanes.flatMap((l) => (l.daily ?? []).map((_, i) => measured(l, i))));
    // Room above the most counted for an estimate on top, as in the opened chart.
    return lanes.some((l) => l.parts?.some((p) => CLAUDE[p.key]?.estimate)) ? Math.min(Math.max(...lanes.flatMap((l) => l.daily ?? []), counted), counted * 1.25) : counted;
  });
  /** Claude's bar for a day: its parts from the bottom up (terminal, cloud, estimate) as shares
   *  of the bar drawn. A day the estimate lifts over the scale is drawn full and cut. */
  function claudeBar(l: Lane, i: number) {
    const v = l.daily?.[i] ?? 0;
    const cut = v > top;
    const parts = (l.parts ?? []).map((p) => ({ key: p.key, v: p.daily?.[i] ?? 0, ...CLAUDE[p.key] })).filter((p) => p.v > 0);
    const real = parts.filter((p) => !p.estimate).reduce((n, p) => n + p.v, 0);
    return {
      v,
      cut,
      h: cut ? 100 : Math.max((v / top) * 100, 3),
      parts: parts.map((p) => ({ ...p, grow: cut ? (p.estimate ? Math.max(100 - (real / top) * 100, 8) : (p.v / top) * 100) : (p.v / Math.max(v, 1e-9)) * 100 })).reverse(),
    };
  }
  const share = (n: number, of: number) => (of > 0 ? Math.round((n / of) * 100) : 0);
  const pct = (n: number, of: number) => {
    const p = of > 0 ? (n / of) * 100 : 0;
    return p === 0 ? "0%" : p < 1 ? "<1%" : `${Math.round(p)}%`;
  };
  /** An amount in the page's unit. Money paid by the token (DeepSeek, Jev) stays in dollars. */
  const amount = (n: number) => (shownTokens ? fmtTokens(n) : dollars(n));
  /** A ≈ for an unknown price or an estimate; a count of tokens is never rough. */
  const approx = (l: Lane) => !shownTokens && !l.priced;
  const dollars = (n: number) => (n >= 100 ? `$${Math.round(n)}` : n >= 10 ? `$${n.toFixed(0)}` : n > 0 ? `$${n.toFixed(n < 1 ? 2 : 1)}` : "$0");
  const [whole, cents] = $derived(shownTokens ? [fmtTokens(total), ""] : total.toFixed(2).split("."));

  /** Hub's bars: each day's dollars per engine, biggest engine at the bottom. */
  function stack(day: number) {
    return (hubLane?.engines ?? []).map((e) => ({ key: e.key, cost: e.daily[day] ?? 0 })).filter((e) => e.cost > 0);
  }

  const plan = (engine: string | undefined) => {
    if (!engine) return null;
    const sub = hub.subscriptions.find((s) => s.engine === engine);
    return sub ? shownPlan(engine, sub.plan?.plan) : null;
  };

  const who = $derived.by(() => {
    const sources = full && spend ? spend.sources : [];
    const mine = sources.filter((s) => YOURS.has(s.key)).reduce((n, s) => n + s.cost, 0);
    const own = sources.filter((s) => !YOURS.has(s.key)).reduce((n, s) => n + s.cost, 0);
    return { mine, own, known: sources.length > 0 };
  });

  const deepseek = $derived(hub.subscriptions.find((s) => s.balance != null) ?? null);
  const deepseekSpent = $derived(full && spend && !shownTokens ? (hubLane?.engines.find((e) => e.key === "deepseek")?.cost ?? 0) : null);
  /** The first day counted, for "All time". */
  const since = $derived(days === ALL && spend?.dates[0] ? new Date(`${spend.dates[0]}T12:00:00`).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" }) : null);
  const rangeWord = $derived(days === 1 ? "today" : days === ALL ? (since ? `since ${since}` : "all time") : `last ${days} days`);

  /** The slips' purposes, in four groups: what you wrote, handovers, Hub's own words in a thread
   *  and the quick one-shot calls (titles, summaries and the like). */
  const SENT_GROUPS = [
    { key: "you", icon: "user", name: "Prompts", sub: "your messages, with what Hub put around them" },
    { key: "handover", icon: "swap", name: "Handovers", sub: "a thread switched, rewound or moved" },
    { key: "hub", icon: "robot", name: "Hub in a thread", sub: "reviews, quota notes and other words of Hub's" },
    { key: "helper", icon: "spark", name: "Quick helper calls", sub: "" },
  ] as const;
  const groupOf = (purpose: string) =>
    purpose === "message" ? "you" : ["switch", "rewind", "moved"].includes(purpose) ? "handover" : purpose.startsWith("hub:") ? "hub" : "helper";
  /** What a one-shot is called on the page. */
  const HELPERS: Record<string, string> = {
    title: "titles", gist: "summaries", notes: "patch notes", review: "reviews", review_split: "reviews", tidy: "tidying",
    judge_answer: "the judge", commit_draft: "commit messages", polish: "polishing", day_summary: "day summaries",
    customs_review: "Customs", design_check: "design checks", focus_check: "focus checks", handoff: "handoffs",
    tool_handoff: "handoffs", idea_place: "ideas", jev_tune: "Jev tuning", tool_answers: "tool answers", usage_probe: "usage probes",
  };
  const sentRows = $derived.by(() => {
    if (!sent) return null;
    const groups = new Map<string, { calls: number; tokens: number; parts: Map<string, number> }>();
    for (const d of sent.days)
      for (const p of d.purposes) {
        const g = groups.get(groupOf(p.purpose)) ?? { calls: 0, tokens: 0, parts: new Map() };
        g.calls += p.calls;
        g.tokens += p.tokens;
        const name = HELPERS[p.purpose] ?? p.purpose.replace(/^hub:/, "").replace(/_/g, " ");
        g.parts.set(name, (g.parts.get(name) ?? 0) + p.tokens);
        groups.set(groupOf(p.purpose), g);
      }
    const all = [...groups.values()].reduce((n, g) => n + g.tokens, 0);
    const rows = SENT_GROUPS.map((about) => {
      const g = groups.get(about.key);
      const biggest = g ? [...g.parts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 3).map(([n]) => n) : [];
      return { ...about, sub: about.key === "helper" ? biggest.join(", ") || "titles, summaries, patch notes" : about.sub, calls: g?.calls ?? 0, tokens: g?.tokens ?? 0 };
    });
    return { all, mine: rows[0].tokens, rows };
  });
  const tokens = (n: number) => (n >= 1_000_000 ? `${(n / 1_000_000).toFixed(1)} M` : n >= 1000 ? `${Math.round(n / 1000)} k` : `${n}`);

  /** The connection Hub's own decisions ask (Jev), and what it cost in the month or today. */
  const jev = $derived.by(() => {
    if (!apis) return null;
    const asks = new Set(apis.catalog.filter((k) => k.asks).map((k) => k.kind));
    const c = apis.prefs.connections.filter((c) => (apis?.prefs.helper ?? c.name) === c.name).find((c) => asks.has(c.kind));
    if (!c) return null;
    const s = apis.status.find((s) => s.name === c.name);
    const used = days === 1 ? s?.today : s?.month;
    const price = API_PRICE[c.kind] ?? null;
    return {
      kind: c.kind,
      label: c.label || c.name,
      brand: BRANDS[c.kind] ?? "var(--accent)",
      ok: s?.check ? s.check.ok : null,
      calls: used?.calls ?? 0,
      usd: price === null ? null : ((used?.input_tokens ?? 0) * price) / 1_000_000,
    };
  });
  /** Jev's dollars: the first two decimals plain, the rest in its colour, the way its card ticks. */
  const jevMoney = (n: number) => {
    const s = n.toFixed(n < 1 ? 4 : 2);
    const at = s.indexOf(".") + 3;
    return [s.slice(0, at), s.slice(at)];
  };
</script>

<div class="where">
  <div class="top">
    <div class="ranges" role="group" aria-label="Days">
      {#each RANGES as r (r.days)}
        {@const later = r.days === ALL && !canAll}
        <button class="btn" class:primary={days === r.days} disabled={later} data-tip={later ? "Comes with the next version of Hub" : null}
          onclick={() => (days = r.days)}>{r.label}</button>
      {/each}
    </div>
    <div class="ranges unit" role="group" aria-label="Count in">
      {#each [{ u: "dollars", label: "$" }, { u: "tokens", label: "Tokens" }] as const as o (o.u)}
        <button class="btn" class:primary={(o.u === "tokens") === inTokens} disabled={o.u === "tokens" && !canTokens}
          data-tip={o.u === "tokens" && !canTokens ? "Comes with the next version of Hub" : o.u === "dollars" ? "What the same tokens would cost through the API" : "Fresh input and output, without cache reads"}
          onclick={() => pickUnit(o.u)}>{o.label}</button>
      {/each}
    </div>
  </div>

  {#if failed}
    <p class="note">Could not read the numbers: {failed}</p>
  {:else if !lanes.length}
    <Quiet kind="loading" />
  {:else}
    <div class="total">
      {#if shownTokens}
        <b>{whole}<span class="cents" class:moving={running > 0}>&nbsp;tokens</span></b>
      {:else}
        <b>${whole}<span class="cents" class:moving={running > 0}>.{cents}</span></b>
      {/if}
      {#if running}<span class="live">live · {running} {running === 1 ? "thread" : "threads"} running</span>{/if}
    </div>
    {#if machines}
      <div class="machines" aria-label="Per machine">
        {#each machines as m (m.name)}
          <span class:off={!!m.error} data-tip={m.error ? `Not counted: ${m.error}` : `${fmtTokens(m.tokens)} tokens`}>
            <Icon name="monitor" size={12} />{m.name}<b>{m.error ? m.error : amount(m.cost)}</b>
          </span>
        {/each}
      </div>
    {/if}

    <div class="cards">
      {#each lanes as l (l.key)}
        {@const about = LANES[l.key] ?? { name: l.key, sub: "", mark: "hub" }}
        {@const p = plan(about.engine)}
        <button class="card {l.key}" class:open={open === l.key} class:can={!!l.daily} aria-expanded={l.daily ? open === l.key : undefined}
          onclick={() => l.daily && (open = open === l.key ? null : l.key)}>
          <i class="fx" aria-hidden="true"></i>
          <div class="head">
            {#if about.mark === "hub"}<span class="hubmark"><Icon name="hub" size={13} /></span>{:else}<ProviderMark id={about.mark} size={16} />{/if}
            <b>{about.name}</b>
            {#if p}<span class="plan">{p}</span>{/if}
            <span class="amt" data-tip={approx(l) ? roughly(l) : null}>{approx(l) ? "≈" : ""}{amount(l.cost)}</span>
          </div>
          {#if account(l.key)}
            {@const a = account(l.key)!}
            <!-- In place of the line under the name, so every card keeps its height. -->
            <div class="sub" data-tip="{about.sub}. Codex's own count of your whole account, wherever it ran{a.lifetime ? ` · ${fmtTokens(a.lifetime)} since the start` : ''}">
              whole account: <b>{fmtTokens(a.tokens)} tokens</b>
            </div>
          {:else}
            <div class="sub">{about.sub}</div>
          {/if}
          {#if l.daily}
            <div class="chart">
            {#if l.key === "hub" && l.engines.length > 1}
              <div class="key">
                {#each l.engines as e (e.key)}<span><i style:background={ENGINES[e.key]?.colour ?? "var(--text-3)"}></i>{ENGINES[e.key]?.name ?? e.key}</span>{/each}
              </div>
            {:else if l.parts && l.parts.length > 1}
              <div class="key">
                {#each l.parts as p (p.key)}<span><i class:est={CLAUDE[p.key]?.estimate} style:--c={CLAUDE[p.key]?.rgb}></i>{CLAUDE[p.key]?.name ?? p.key}</span>{/each}
              </div>
            {/if}
            <div class="week" aria-label="Per day">
              {#each l.daily as v, i (i)}
                {@const h = Math.max((v / top) * 100, 3)}
                {#if l.parts}
                  {@const b = claudeBar(l, i)}
                  <span class="bar stack" class:cut={b.cut} class:now={i === l.daily.length - 1} style:height="{b.h}%"
                    data-tip="{spend?.dates[i] ?? ''} · {b.parts.some((p) => p.estimate) ? '≈' : ''}{amount(v)}">
                    {#each b.parts as p (p.key)}<i class:est={p.estimate} style:flex-grow={p.grow} style:--c={p.rgb}></i>{/each}
                  </span>
                {:else if l.key === "hub" && l.engines.length}
                  <span class="bar stack" class:now={i === l.daily.length - 1} style:height="{h}%" data-tip="{spend?.dates[i] ?? ''} · {amount(v)}">
                    {#each stack(i).reverse() as e (e.key)}<i style:flex-grow={(e.cost / Math.max(v, 1e-9)) * 100} style:--c={ENGINES[e.key]?.rgb ?? "189, 184, 176"}></i>{/each}
                  </span>
                {:else}
                  <span class="bar" class:now={i === l.daily.length - 1} style:height="{h}%" data-tip="{spend?.dates[i] ?? ''} · {amount(v)}"></span>
                {/if}
              {/each}
            </div>
            </div>
          {:else}
            <div class="week none">{l.key === "codex" ? "Codex is counted from the next version" : "Days come with the next version"}</div>
          {/if}
          <div class="row"><span>Share of all</span><span class="meter"><i style:width="{share(l.cost, total)}%"></i></span><em>{pct(l.cost, total)}</em></div>
        </button>
      {/each}
    </div>

    {#if opened}
      {@const about = LANES[opened.key] ?? { name: opened.key, sub: "", mark: "hub" }}
      {@const real = (opened.daily ?? []).map((_, i) => measured(opened, i))}
      {@const busiest = real.indexOf(Math.max(...real, 0))}
      <div class="detail {opened.key}">
        <i class="fx" aria-hidden="true"></i>
        <div class="head">
          {#if about.mark === "hub"}<span class="hubmark"><Icon name="hub" size={13} /></span>{:else}<ProviderMark id={about.mark} size={16} />{/if}
          <b>{about.name}</b><span class="dsub">{rangeWord}</span>
          <button class="btn close" aria-label="Close" onclick={() => (open = null)}><Icon name="x" size={14} /></button>
        </div>
        <div class="dbody">
          {#if zoomData || per === "day"}
            <WhereZoom data={zoomData ?? spend!} lane={opened.key} group={opened.parts ? CLAUDE : undefined} range={days} engines={ENGINES} sources={SOURCES} {amount} />
          {:else}
            <Quiet kind="loading" />
          {/if}
          <div class="facts">
            <div class="kv"><span>{shownTokens ? "Tokens" : "Spent"}</span><b>{approx(opened) ? "≈" : ""}{amount(opened.cost)}</b></div>
            {#if opened.parts}
              <!-- Claude's parts in place of answers and tokens, so its view is as tall as the others. -->
              {#each opened.parts as part (part.key)}
                {@const about = CLAUDE[part.key]}
                <div class="row r3" data-tip={approx(part) ? roughly(part) : null}>
                  <span><i class="dot" class:est={about?.estimate} style:--c={about?.rgb}></i>{about?.name ?? part.key}</span>
                  <span class="meter"><i style:width="{share(part.cost, opened.cost)}%" style:background="rgb({about?.rgb})"></i></span>
                  <em class="pc">{pct(part.cost, opened.cost)}</em><em>{approx(part) ? "≈" : ""}{amount(part.cost)}</em>
                </div>
              {/each}
              <div class="sep"></div>
            {:else}
              <div class="kv"><span>Answers</span><b>{answers(opened) ?? "–"}</b></div>
              <div class="kv"><span>Tokens, without cache reads</span><b>{opened.tokens === null || ESTIMATED.has(opened.key) ? "–" : fmtTokens(opened.tokens)}</b></div>
            {/if}
            <div class="kv"><span>A day, on average</span><b>{amount(opened.cost / Math.max(1, spend?.days ?? opened.daily?.length ?? 1))}</b></div>
            <div class="kv"><span>Busiest {per}{opened.parts?.some((p) => !p.priced) ? ", measured" : ""}</span><b>{busiest >= 0 ? `${day(spend?.dates[busiest])} · ${amount(real[busiest] ?? 0)}` : "–"}</b></div>
            {#if opened.key === "hub" && spend}
              <div class="sep"></div>
              {@const ranked = [...spend.sources].sort((a, b) => b.cost - a.cost)}
              {@const rest = ranked.slice(TOP_SOURCES)}
              {#each everySource || rest.length < 2 ? ranked : ranked.slice(0, TOP_SOURCES) as src (src.key)}
                <div class="row r3">
                  <span>{SOURCES[src.key] ?? src.key}</span>
                  <span class="meter"><i style:width="{share(src.cost, opened.cost)}%"></i></span>
                  <em class="pc">{pct(src.cost, opened.cost)}</em><em>{amount(src.cost)}</em>
                </div>
              {/each}
              {#if rest.length >= 2}
                <!-- The rest folded into one row, so Hub's view is as tall as the others. -->
                <button class="row r3 more" onclick={() => (everySource = !everySource)}>
                  <span>{everySource ? "Fewer" : `${rest.length} more`}</span><span></span>
                  <em class="pc">{everySource ? "" : pct(rest.reduce((n, r) => n + r.cost, 0), opened.cost)}</em><em>{everySource ? "" : amount(rest.reduce((n, r) => n + r.cost, 0))}</em>
                </button>
              {/if}
            {/if}
          </div>
        </div>
      </div>
    {/if}

    <div class="lower">
      <div class="box">
        <div class="bh"><span class="hubmark"><Icon name="hub" size={13} /></span><span>Inside Hub</span><b>{amount(hubLane?.cost ?? 0)}</b></div>
        {#if who.known}
          {#each [{ icon: "user", name: "You asked", v: who.mine }, { icon: "robot", name: "Hub on its own", v: who.own }] as r (r.name)}
            <div class="kv" data-tip="{pct(r.v, hubLane?.cost ?? 0)} of Hub"><span class="lbl"><Icon name={r.icon as "user"} size={14} />{r.name}</span><b>{amount(r.v)}</b></div>
          {/each}
        {:else}
          <p class="hint">Who asked comes with the next version.</p>
        {/if}
        {#if hubLane?.engines.length}
          <div class="split" aria-hidden="true">
            {#each hubLane.engines as e (e.key)}<i style:flex-grow={Math.max(e.cost, 1e-9)} style:background={ENGINES[e.key]?.colour ?? "var(--text-3)"}></i>{/each}
          </div>
          <div class="chips">
            {#each hubLane.engines as e (e.key)}
              {@const about = ENGINES[e.key]}
              <span class="chip" data-tip="{pct(e.cost, hubLane.cost)} of Hub">{#if about}<ProviderMark id={about.mark} size={13} />{/if}{about?.name ?? e.key}<b>{amount(e.cost)}</b></span>
            {/each}
          </div>
        {/if}
      </div>

      {#if deepseek}
        <div class="box paid">
          <div class="bh"><ProviderMark id="deepseek" size={15} /><b class="name">DeepSeek</b><span class="aside">paid per token</span></div>
          <div class="kv"><span>Balance left</span><b>{deepseek.balance}</b></div>
          {#if deepseekSpent !== null}
            <div class="kv"><span>Spent, {rangeWord}</span><b>{dollars(deepseekSpent)}</b></div>
          {:else if deepseek.spent_today}
            <div class="kv"><span>Spent today</span><b>{deepseek.spent_today}</b></div>
          {/if}
        </div>
      {/if}

      {#if jev}
        <div class="box paid jev" style:--brand={jev.brand}>
          <div class="jh">
            <span class="wash"></span>
            <ProviderLogo id={jev.kind} label={jev.label} size={15} />
            <b>{jev.label}</b>
            {#if jev.ok !== null}<span class="state" class:bad={!jev.ok}>● {jev.ok ? "Works" : "Failing"}</span>{/if}
            <span class="aside">{jev.kind === "typesafe" ? "TypeSafe" : ""}</span>
          </div>
          <div class="kv">
            <span>Spent, {days === 1 ? "today" : "this month"}</span>
            {#if jev.usd !== null}{@const [a, b] = jevMoney(jev.usd)}<b>${a}<span class="tick">{b}</span></b>{:else}<b>–</b>{/if}
          </div>
          <div class="kv"><span>Calls</span><b>{jev.calls}</b></div>
        </div>
      {/if}
    </div>

    {#if sentRows && sentRows.all > 0}
      <div class="box sent">
        <div class="bh">
          <Icon name="bag" size={14} /><span>What Hub sent</span>
          <span class="aside">{tokens(sentRows.all)} tokens to the AIs, {rangeWord}</span>
        </div>
        <div class="split" aria-hidden="true">
          {#each sentRows.rows as r, i (r.key)}<i style:flex-grow={Math.max(r.tokens, 1e-9)} style:opacity={1 - i * 0.22} data-tip="{r.name} · {pct(r.tokens, sentRows.all)}"></i>{/each}
        </div>
        {#each sentRows.rows as r, i (r.key)}
          <div class="sline" data-tip="{pct(r.tokens, sentRows.all)} of what Hub sent">
            <i class="sw" style:opacity={1 - i * 0.22}></i>
            <span class="nm">{r.name}<small>{r.sub}</small></span>
            <em>{r.calls} {r.calls === 1 ? "call" : "calls"}</em><b>{tokens(r.tokens)}</b>
          </div>
        {/each}
      </div>
    {/if}

    <p class="foot">
      Plan use is what the same tokens would cost through the API; DeepSeek and {jev?.label ?? "API connections"} are money really spent.
      {#if full === false}This Hub counts Claude only; Codex, DeepSeek and the days come with the next version.{/if}
      {#if spend?.notes?.length}{#each spend.notes as n (n)}<br />{n}{/each}{/if}
    </p>
  {/if}
</div>

<style>
  /* The tab or row already says "Where it goes", so Settings' heading and subtitle above the page are
     hidden here. Settings itself still renders them; they go from its source with the Settings rework. */
  :global(.inner > h1:has(+ .lead + .where), .inner > .lead:has(+ .where)) { display: none; }
  .where { display: flex; flex-direction: column; container-type: inline-size; }
  .top { display: flex; align-items: center; gap: 12px; margin: 4px 0 16px; }
  .ranges { display: flex; gap: 6px; }
  .ranges.unit { margin-left: auto; }
  .note { color: var(--muted); }

  .total { display: flex; align-items: flex-end; gap: 14px; margin-bottom: 14px; }
  .total b { font-size: 34px; font-weight: 500; line-height: 1; color: var(--text); letter-spacing: -0.02em; font-variant-numeric: tabular-nums; }
  .cents { color: var(--text-3); }
  .cents.moving { color: color-mix(in srgb, var(--accent) 70%, var(--text)); }
  .cap { color: var(--muted); font-size: 13px; line-height: 1.35; padding-bottom: 2px; }
  .live { margin-left: auto; align-self: center; display: inline-flex; align-items: center; gap: 6px; color: color-mix(in srgb, var(--accent) 75%, var(--text)); font-size: 12px; }
  .live::before { content: ""; width: 6px; height: 6px; border-radius: 50%; background: var(--accent); box-shadow: 0 0 0 3px var(--accent-bg); }

  /* The opened card grows to twice the others, so its bars and key have room. */
  .cards { display: flex; gap: 10px; margin-bottom: 12px; }
  .cards > .card { flex: 1 1 0; transition: flex-grow var(--dur-move) var(--ease-move); }
  .cards > .card.hub { flex-grow: 2; }
  .cards > .card.open { flex-grow: 2; }
  /* Five across leaves no room for the plan beside the name. */
  .cards:has(> :nth-child(5)) .plan { display: none; }
  .machines { display: flex; flex-wrap: wrap; gap: 6px 14px; margin: -4px 0 14px; color: var(--muted); font-size: 12px; }
  .machines span { display: inline-flex; align-items: center; gap: 4px; }
  .machines b { color: var(--text-2); font-weight: 500; font-variant-numeric: tabular-nums; }
  .machines .off b { color: var(--text-3); font-weight: 400; }
  .sub b { color: var(--text-2); font-weight: 500; }
  .card { all: unset; box-sizing: border-box; display: block; cursor: default; position: relative; isolation: isolate; overflow: hidden; border-radius: var(--r-md); background: var(--surface); padding: 12px 14px; min-width: 0; }
  /* The limits card's glow, in each company's colours. */
  .fx { display: none; position: absolute; inset: 0; z-index: -1; pointer-events: none; }
  .fx::before, .fx::after { content: ""; position: absolute; width: 190px; height: 190px; border-radius: 50%; filter: blur(40px); }
  .claude .fx::before, .claude_code .fx::before { background: rgba(217, 119, 87, 0.3); left: -60px; top: 10px; }
  .claude .fx::after { background: rgba(120, 160, 220, 0.12); right: -70px; top: -90px; }
  .claude_code .fx::after { background: rgba(204, 160, 112, 0.16); right: -70px; top: -90px; }
  .codex .fx::before { background: rgba(110, 140, 255, 0.24); left: -50px; top: -70px; }
  .codex .fx::after { background: rgba(150, 120, 255, 0.2); right: -60px; top: 30px; }
  .hub .fx::before { background: rgba(236, 232, 226, 0.05); left: -60px; top: -40px; }
  .head { display: flex; align-items: center; gap: 8px; color: var(--text); font-size: 14px; min-width: 0; }
  .head b { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .head .amt { flex: none; }
  .plan { font-size: 12px; color: var(--muted); }
  .amt { margin-left: auto; font-size: 18px; font-weight: 500; font-variant-numeric: tabular-nums; }
  .sub { color: var(--muted); font-size: 12px; margin: 3px 0 8px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .hubmark { display: inline-grid; place-items: center; width: 18px; height: 18px; border-radius: var(--r-sm); background: var(--text-2); color: var(--bg); flex: none; }
  .card.can { cursor: pointer; }
  .card.can:hover { background: color-mix(in srgb, var(--surface) 92%, var(--text)); }
  .card.open { box-shadow: inset 0 0 0 1px var(--line-strong); }
  .card:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  /* Hub's key stands beside its bars, so every card's bars get the same height. */
  .chart { display: flex; gap: 10px; align-items: flex-end; }
  .chart .week { flex: 1; min-width: 0; }
  .key { display: flex; flex-direction: column; gap: 3px; color: var(--muted); font-size: 11px; margin-bottom: 6px; }
  .key span { display: flex; align-items: center; gap: 4px; }
  .key i, i.dot { width: 7px; height: 7px; border-radius: 2px; display: block; flex: none; background: rgb(var(--c, 189, 184, 176)); }
  /* An estimate is striped, never lit: it is a guess, not a count. */
  .key i.est, i.dot.est { background: repeating-linear-gradient(135deg, rgba(var(--c), 0.9) 0 1.5px, transparent 1.5px 3px); box-shadow: inset 0 0 0 1px rgba(var(--c), 0.7); }
  .week { display: flex; align-items: flex-end; justify-content: center; gap: 4px; height: 64px; margin: 2px 0 6px; }
  .week .bar { max-width: 36px; }
  .week.none { align-items: center; color: var(--muted); font-size: 12px; }
  /* Edge bars: a flat bar in the place's own colour with a solid top line, like the limits card's
     Edge theme. --a is the colour of the bar, --b where its lit edge runs to. */
  .card, .detail { --a: 236, 232, 226; --b: 236, 232, 226; }
  .claude, .claude_code { --a: 217, 119, 87; --b: 204, 160, 112; }
  .codex { --a: 110, 140, 255; --b: 150, 120, 255; }
  /* A flat tint of the place's own colour in the card: clay for Claude, blue for Codex, Hub plain. */
  .card.claude { background: color-mix(in srgb, rgb(217, 119, 87) 11%, var(--surface)); }
  .card.codex { background: color-mix(in srgb, rgb(110, 140, 255) 13%, var(--surface)); }
  .card.can.claude:hover { background: color-mix(in srgb, rgb(217, 119, 87) 15%, var(--surface)); }
  .card.can.codex:hover { background: color-mix(in srgb, rgb(110, 140, 255) 17%, var(--surface)); }
  .bar { flex: 1; position: relative; overflow: hidden; border-radius: 3px 3px 2px 2px; background: rgba(var(--a), 0.3); box-shadow: inset 0 0 0 1px rgba(var(--a), 0.35); }
  .bar::before { content: ""; position: absolute; left: 0; right: 0; top: 0; height: 2px; background: rgb(var(--a)); }
  .bar.now { background: rgba(var(--a), 0.5); }
  .bar.stack { display: flex; flex-direction: column; gap: 1px; overflow: hidden; background: none; box-shadow: none; opacity: 0.85; }
  .bar.stack::before { content: none; }
  .bar.stack.now { opacity: 1; }
  /* Hub's bars: each engine its own flat piece in its colour, a solid line along its top. */
  /* Grow in parts of a hundred: shares under 1 would leave the bar part empty. */
  .bar.stack i { display: block; flex: 0 0 0; min-height: 1.5px; background: rgba(var(--c), 0.3); box-shadow: inset 0 2px 0 rgb(var(--c)), inset 0 0 0 1px rgba(var(--c), 0.3); }
  .bar.stack i.est { background: repeating-linear-gradient(135deg, rgba(var(--c), 0.38) 0 3px, rgba(var(--c), 0.08) 3px 7px); box-shadow: inset 0 0 0 1px rgba(var(--c), 0.5); }
  /* A bar the estimate lifts over the scale is drawn full and cut near its top. */
  .bar.stack.cut::after { content: ""; position: absolute; left: -2px; right: -2px; top: 5px; height: 2px; background: var(--surface); transform: skewY(-14deg); }

  .row { display: grid; grid-template-columns: auto 1fr 40px; gap: 12px; align-items: center; color: var(--muted); font-size: 13px; margin-top: 6px; }
  .row > span:first-child { display: flex; align-items: center; gap: 6px; white-space: nowrap; }
  .row em { font-style: normal; color: var(--text-2); text-align: right; font-variant-numeric: tabular-nums; }
  .row em.pc { color: var(--muted); }
  .row.more { all: unset; box-sizing: border-box; display: grid; grid-template-columns: 140px 1fr 44px 48px; gap: 12px; align-items: center; width: 100%; margin-top: 8px; color: var(--muted); font-size: 13px; cursor: pointer; }
  .row.more:hover span:first-child { color: var(--text-2); }
  .row.more:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; border-radius: var(--r-xs); }
  .row.more em { font-style: normal; text-align: right; font-variant-numeric: tabular-nums; color: var(--text-2); }
  .row.r3 { grid-template-columns: 140px 1fr 44px 48px; margin-top: 8px; }
  .meter { height: 4px; border-radius: 2px; background: rgba(255, 255, 255, 0.08); display: block; }
  .meter i { display: block; height: 100%; border-radius: 2px; background: var(--text-2); }

  .detail { position: relative; isolation: isolate; overflow: hidden; border-radius: var(--r-md); background: var(--surface); padding: 12px 14px 14px; margin-bottom: 12px; box-shadow: inset 0 0 0 1px var(--line-strong); }
  .detail .head { margin-bottom: 10px; }
  .dsub { color: var(--muted); font-size: 12px; }
  .close { margin-left: auto; padding: 3px 6px; }
  .dbody { display: grid; grid-template-columns: minmax(0, 1.7fr) minmax(0, 1fr); gap: 22px; }
  .facts .kv:first-child { margin-top: 0; }
  @container (max-width: 700px) { .dbody { grid-template-columns: minmax(0, 1fr); } }

  .lower { display: grid; grid-template-columns: minmax(0, 1.6fr) minmax(0, 1fr) minmax(0, 1fr); gap: 10px; align-items: start; }
  /* Narrower: Hub's split gets the whole row and the paid cards share the next; narrower still, one column. */
  @container (max-width: 900px) {
    .lower { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
    .lower > :first-child { grid-column: 1 / -1; }
  }
  @container (max-width: 600px) {
    .cards, .lower { grid-template-columns: minmax(0, 1fr); }
  }
  .box { background: var(--surface); border-radius: var(--r-md); padding: 12px 14px; min-width: 0; }
  .bh { display: flex; align-items: center; gap: 8px; color: var(--text); font-size: 14px; margin-bottom: 4px; }
  .bh b { margin-left: auto; font-weight: 500; }
  .bh b.name { margin-left: 0; font-weight: 600; }
  .aside { margin-left: auto; color: var(--muted); font-size: 12px; white-space: nowrap; }
  .sep { border-top: 1px solid var(--line); margin: 10px 0 2px; }
  .hint { color: var(--muted); font-size: 12px; margin: 4px 0 6px; }
  /* One slim strip split by part, with 2px gaps; the rows or chips beside it name the parts. */
  .split { display: flex; gap: 2px; height: 6px; margin: 10px 0 8px; }
  .split i { display: block; min-width: 3px; border-radius: var(--r-xs); background: var(--text-2); }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { display: inline-flex; align-items: center; gap: 6px; padding: 3px 8px; border-radius: var(--r-xs); background: color-mix(in srgb, var(--text) 6%, transparent); color: var(--muted); font-size: 12px; }
  .chip b { color: var(--text); font-weight: 500; font-variant-numeric: tabular-nums; }
  .kv .lbl { display: inline-flex; align-items: center; gap: 6px; }
  .kv { display: flex; justify-content: space-between; color: var(--muted); font-size: 13px; margin-top: 8px; }
  .kv b { color: var(--text); font-weight: 500; font-variant-numeric: tabular-nums; }

  /* Jev's card: its dithered cloud behind the head, as in the status bar. */
  .jev { padding: 0 0 12px; overflow: hidden; }
  .jev .kv { padding: 0 14px; }
  .jh { position: relative; display: flex; align-items: center; gap: 8px; padding: 10px 14px 8px; font-size: 13px; color: var(--text); }
  .jh > :not(.wash) { position: relative; }
  .jh b { font-weight: 600; }
  .wash { position: absolute; inset: 0; opacity: 0.24; pointer-events: none; background: radial-gradient(ellipse 40% 120% at 0% 50%, #e650bb, transparent 70%), radial-gradient(ellipse 40% 120% at 100% 30%, var(--brand), transparent 70%), linear-gradient(90deg, transparent, #8fc3dc66); }
  .wash::after { content: ""; position: absolute; inset: 0; background-image: radial-gradient(rgba(0, 0, 0, 0.5) 0.8px, transparent 1.1px); background-size: 3px 3px; }
  .state { font-size: 12px; color: var(--green); white-space: nowrap; }
  .state.bad { color: var(--red); }
  .tick { color: color-mix(in srgb, #e650bb 45%, var(--text)); }

  .sent { margin-top: 10px; }
  .sline { display: grid; grid-template-columns: 8px minmax(0, 1fr) auto 56px; gap: 12px; align-items: center; margin-top: 10px; color: var(--muted); font-size: 13px; }
  .sline em { font-style: normal; text-align: right; }
  .sline b { color: var(--text); font-weight: 500; text-align: right; font-variant-numeric: tabular-nums; }
  .sw { width: 8px; height: 8px; border-radius: 2px; background: var(--text-2); }
  .nm { display: flex; flex-direction: column; min-width: 0; color: var(--text-2); }
  .nm small { color: var(--muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; }
  @container (max-width: 600px) { .sline em { display: none; } .sline { grid-template-columns: 8px minmax(0, 1fr) 56px; } }

  .foot { color: var(--muted); font-size: 13px; margin: 14px 0 0; }
</style>
