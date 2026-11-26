<!-- Days as dots, lit by how much you worked: Hub's threads, Claude Code outside Hub, and your commits locally and on GitHub. As many weeks as fit the card (up to two years), so a wide card has no empty half; the totals are always the last twelve months. Click a day to see it in the Day card (DayCard.svelte).
     Used by CardBody.svelte. -->
<script lang="ts">
  // Days as dots, lit by how much you worked: Hub's threads, Claude Code outside Hub, and your
  // commits locally and on GitHub. As many weeks as fit the card (up to two years), so a wide
  // card has no empty half; the totals are always the last twelve months. Click a day to see it
  // in the Day card (DayCard.svelte).
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { ActivityDay } from "../bindings/ActivityDay";
  import { recall, remember } from "../lastSeen";
  import { knob, overview, pickDay } from "../overview.svelte";
  import { prefs } from "../prefs.svelte";
  import { snap, snapDown } from "../pixels.svelte";
  import { TEXT_MIN } from "../textSize";
  import { hub } from "../state.svelte";
  import HowCounted from "./HowCounted.svelte";

  /** The most weeks the map draws, and what is asked for once. */
  const MAX_WEEKS = 106;
  /** The totals and streaks: the last 53 weeks, whatever the map shows. */
  const YEAR = 53 * 7;
  /** The width a day aims for (gap included) when deciding how many weeks fit. */
  const PITCH = 17;
  const tz = -new Date().getTimezoneOffset();

  /** The sources switched on in the card's menu: Hub, Claude Code, Git and GitHub. */
  const sources = $derived.by(() => {
    void prefs.overviewFrames;
    return { hub: knob<boolean>("map", "hub"), claude: knob<boolean>("map", "claude"), git: knob<boolean>("map", "git") };
  });

  let days = $state<Record<string, ActivityDay>>({});
  let notes = $state<string[]>([]);
  let loaded = $state(false);
  const selected = $derived(overview.picked.map ?? null);

  const key = (d: Date) =>
    `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  const today = key(new Date());
  /** Local commits and GitHub's commits overlap, so a day counts whichever is larger. GitHub's
   *  other contributions (pull requests, reviews, issues) are not commits. */
  const commitsOf = (d: ActivityDay) => Math.max(d.commits, d.github_commits ?? 0);
  const score = (d: ActivityDay | undefined) =>
    d
      ? (sources.hub ? d.messages + d.turns : 0) + (sources.claude ? d.prompts * 2 : 0) + (sources.git ? commitsOf(d) * 2 : 0)
      : 0;
  const tokens = (n: number) =>
    n >= 1e9 ? `${(n / 1e9).toFixed(1)}B` : n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}k` : `${n}`;

  /** Columns of Monday-first weeks, ending with this one. Days after today are null. */
  const allWeeks = (() => {
    const now = new Date();
    const start = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    start.setDate(start.getDate() - ((start.getDay() + 6) % 7) - (MAX_WEEKS - 1) * 7);
    const cols: (Date | null)[][] = [];
    for (let w = 0; w < MAX_WEEKS; w++) {
      const col: (Date | null)[] = [];
      for (let i = 0; i < 7; i++) {
        const d = new Date(start.getFullYear(), start.getMonth(), start.getDate() + w * 7 + i);
        col.push(key(d) > today ? null : d);
      }
      cols.push(col);
    }
    return cols;
  })();

  // As many weeks as fit, at least 20, and days that fill the width exactly.
  let width = $state(0);
  const LABELS = 34;
  const shown = $derived(width ? Math.max(20, Math.min(MAX_WEEKS, Math.floor((width - LABELS) / PITCH))) : 53);
  // Whole device pixels (pixels.svelte.ts), so the days line up and the gaps stay even at any zoom.
  const gap = $derived(snap(3));
  const cell = $derived(width ? Math.max(snap(11), Math.min(snap(22), snapDown((width - LABELS) / shown - gap))) : 11);
  const weeks = $derived(allWeeks.slice(-shown));

  /** A month's name over the first week that starts in it, with the year at January when it fits
   *  before the next name (a big text size makes "Jan 2026" wider than four weeks). */
  const months = $derived.by(() => {
    const names = weeks.map((col, i) => {
      const d = col[0];
      const prev = weeks[i - 1]?.[0];
      const next = weeks[i + 1]?.[0];
      if (!d || (prev && prev.getMonth() === d.getMonth())) return "";
      // The first column only gets a name when the next one would not crowd it.
      if (!prev && next && next.getMonth() !== d.getMonth()) return "";
      return d.toLocaleDateString([], { month: "short" });
    });
    // The label's font is 10px grown by the text size (see vite.config.ts); a letter is about 0.6 of it.
    const letter = 0.6 * Math.max(10, (TEXT_MIN[prefs.textSize] ?? 0) - 2.5);
    return names.map((name, i) => {
      const d = weeks[i]?.[0];
      if (!name || !d || d.getMonth() !== 0) return name;
      const at = names.findIndex((n, j) => j > i && n);
      const room = ((at < 0 ? weeks.length : at) - i) * (cell + gap);
      const long = `${name} ${d.getFullYear()}`;
      return room >= long.length * letter + 4 ? long : name;
    });
  });

  /** Four steps, split at the quartiles of the days you were active, so a quiet year still shows shape. */
  const steps = $derived.by(() => {
    const scores = Object.values(days).map(score).filter((s) => s > 0).sort((a, b) => a - b);
    const at = (q: number) => scores[Math.floor((scores.length - 1) * q)] ?? 0;
    return [at(0.25), at(0.5), at(0.75)];
  });
  const level = (date: string) => {
    const s = score(days[date]);
    if (!s) return 0;
    return s <= steps[0] ? 1 : s <= steps[1] ? 2 : s <= steps[2] ? 3 : 4;
  };

  /** The last twelve months' dates, oldest first. */
  const year = (() => {
    const now = new Date();
    return Array.from({ length: YEAR }, (_, i) => key(new Date(now.getFullYear(), now.getMonth(), now.getDate() - (YEAR - 1) + i)));
  })();

  /** Totals and streaks over the last twelve months, from the sources switched on. */
  const stats = $derived.by(() => {
    let longest = 0;
    let run = 0;
    for (const d of year) {
      run = score(days[d]) ? run + 1 : 0;
      longest = Math.max(longest, run);
    }
    // Today without work yet does not break the streak.
    let current = 0;
    for (let i = year.length - 1; i >= 0; i--) {
      if (score(days[year[i]])) current++;
      else if (year[i] !== today) break;
    }
    const all = year.map((d) => days[d]).filter((d): d is ActivityDay => !!d);
    return {
      active: all.filter((d) => score(d)).length,
      current,
      longest,
      prompts: all.reduce((n, d) => n + (sources.hub ? d.messages : 0) + (sources.claude ? d.prompts : 0), 0),
      tokens: all.reduce((n, d) => n + d.tokens, 0),
      commits: sources.git ? all.reduce((n, d) => n + commitsOf(d), 0) : 0,
    };
  });

  // One formatter for every cell: toLocaleDateString builds a new one per call, and the map has
  // hundreds of cells, which made opening System lag.
  const dayName = new Intl.DateTimeFormat([], { weekday: "short", day: "numeric", month: "short" });

  function describe(date: string) {
    const d = days[date];
    const when = dayName.format(new Date(date + "T12:00"));
    if (!d) return `${when}: nothing`;
    const parts = [
      d.messages && `${plural(d.messages, "message")} in ${plural(d.threads, "thread")}`,
      d.prompts && `${plural(d.prompts, "Claude Code prompt")}`,
      d.tokens && `${tokens(d.tokens)} tokens`,
      commitsOf(d) && `${plural(commitsOf(d), "commit")}`,
      d.github > (d.github_commits ?? 0) && `${plural(d.github - (d.github_commits ?? 0), "other GitHub contribution")}`,
    ].filter(Boolean);
    return `${when}: ${parts.join(", ") || "nothing"}`;
  }

  type Report = Awaited<ReturnType<typeof api.activity>>;
  function show(report: Report) {
    notes = report.notes;
    days = Object.fromEntries(report.days.map((d) => [d.date, d]));
  }

  async function load() {
    // Counting two years of history takes a moment; the last count shows until the new one is in.
    const last = recall<Report>(`activity.${MAX_WEEKS * 7}.${tz}`);
    if (last) show(last);
    try {
      const report = await api.activity(MAX_WEEKS * 7, tz);
      show(report);
      remember(`activity.${MAX_WEEKS * 7}.${tz}`, report);
      loaded = true;
      const latest = report.days.at(-1);
      if (!selected && latest) pick(latest.date);
    } catch (e) {
      hub.fail(e);
    }
  }

  // The Day card asks whether a day had anything.
  $effect(() => {
    overview.levels = Object.fromEntries(Object.keys(days).map((d) => [d, level(d)]));
  });

  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

  /** Picks a day: it lights here, and cards linked to the map (the Day card at first) show it. */
  export function pick(date: string) {
    pickDay("map", date);
  }

  onMount(load);
</script>

<HowCounted card="map">
  <p><b>Day streak</b>: days in a row with any work. <b>Longest</b>: the longest run. <b>Active days</b>: days with any work.</p>
  <p><b>Prompts</b>: messages you sent, in Hub and Claude Code. <b>Tokens</b>: fresh input plus output, without cache reads.</p>
  <p><b>Commits</b>: the larger of git and GitHub per day, not added. Numbers cover 12 months.</p>
</HowCounted>

<div class="stats">
  <div><span class="figure">{stats.current}</span><span class="label">day streak</span></div>
  <div><span class="figure">{stats.longest}</span><span class="label">longest</span></div>
  <div><span class="figure">{stats.active}</span><span class="label">active days</span></div>
  <div><span class="figure">{stats.prompts}</span><span class="label">prompts</span></div>
  <div><span class="figure">{tokens(stats.tokens)}</span><span class="label">tokens</span></div>
  <div><span class="figure">{stats.commits}</span><span class="label">commits</span></div>
</div>

<div class="scroll" bind:clientWidth={width}>
  <div class="grid" class:loading={!loaded && !Object.keys(days).length} style:--cell="{cell}px" style:--gap="{gap}px">
    <div class="weekdays">
      <span></span><span>Mon</span><span></span><span>Wed</span><span></span><span>Fri</span><span></span><span></span>
    </div>
    {#each weeks as col, i (i)}
      <div class="week">
        <span class="month">{months[i]}</span>
        {#each col as d, j (j)}
          {#if d}
            {@const date = key(d)}
            {@const tip = describe(date)}
            <button
              class="dot l{level(date)}"
              class:selected={selected === date}
              class:today={date === today}
              data-tip={tip}
              aria-label={tip}
              onclick={() => pick(date)}
            ></button>
          {:else}
            <span class="dot none"></span>
          {/if}
        {/each}
      </div>
    {/each}
  </div>
</div>

{#if notes.length}
  <p class="caption">{notes.join(" · ")}</p>
{/if}

<style>
  /* Six figures in one row however wide the card is: they shrink a little in a narrow one. */
  .stats { display: flex; justify-content: space-between; gap: 10px; margin-bottom: 14px; }
  .stats > div { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .stats :global(.figure) { font-size: clamp(16px, 4.4cqw, 22px); }
  .label { color: var(--muted); font-size: 11px; white-space: nowrap; }

  /* The map shows as many weeks as fit the card, and the days stretch to fill it exactly. */
  .scroll { overflow-x: auto; padding-bottom: 4px; }
  .grid { display: flex; gap: var(--gap, 3px); width: max-content; transition: opacity var(--dur-move) var(--ease); }
  .grid.loading { opacity: 0.4; }
  .weekdays, .week { display: flex; flex-direction: column; gap: var(--gap, 3px); }
  .weekdays { margin-right: 4px; }
  .weekdays span { height: var(--cell); font-size: 11px; line-height: var(--cell); color: var(--muted); }
  .weekdays span:first-child { height: 14px; }
  .month { height: 14px; font-size: 11px; line-height: 11px; color: var(--muted); white-space: nowrap; width: var(--cell); overflow: visible; }

  .dot { width: 11px; height: 11px; border-radius: 3px; display: block; flex: none; }
  .week .dot { width: var(--cell); height: var(--cell); border-radius: calc(var(--cell) / 4); }
  button.dot { cursor: pointer; outline-offset: 1px; }
  button.dot:hover { outline: 1px solid var(--text-3); }
  /* Today: a ring, and a dot in the middle, so the day stands out among the lit ones too. */
  .dot.today { position: relative; outline: 1.5px solid var(--text); }
  .dot.today::after { content: ""; position: absolute; inset: 50% auto auto 50%; width: 3px; height: 3px; margin: -1.5px 0 0 -1.5px; border-radius: 50%; background: var(--text); }
  .dot.selected:not(.today) { outline: 1.5px solid var(--text-3); }
  .dot.none { visibility: hidden; }
  .l0 { background: var(--surface-hover); }
  .l1 { background: color-mix(in srgb, var(--accent) 24%, transparent); }
  .l2 { background: color-mix(in srgb, var(--accent) 45%, transparent); }
  .l3 { background: color-mix(in srgb, var(--accent) 70%, transparent); }
  .l4 { background: var(--accent); }

</style>
