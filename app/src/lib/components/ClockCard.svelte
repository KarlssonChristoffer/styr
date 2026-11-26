<!-- When you work: every message you wrote in Hub and prompt in Claude Code, laid out as hour of the day by weekday, in this computer's clock. With daylight on (a knob in the card's menu), each hour is coloured by how light it is today at your place: violet at night, orange in the twilight, gold in the day, so the map looks different in winter and in summer. Play the last year (in the menu) runs the window over the last year to watch the light move. The line under it names your busiest stretch and how much of your writing was done in daylight, by the sun as it stood when each message was written. The period is a knob too.
     Used by CardBody.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // When you work: every message you wrote in Hub and prompt in Claude Code, laid out as hour of
  // the day by weekday, in this computer's clock. With daylight on (a knob in the card's menu),
  // each hour is coloured by how light it is today at your place: violet at night, orange in the
  // twilight, gold in the day, so the map looks different in winter and in summer. Play the last
  // year (in the menu) runs the window over the last year to watch the light move. The line
  // under it names your busiest stretch and how much of your writing was done in daylight, by
  // the sun as it stood when each message was written. The period is a knob too.
  import { daySlice, light, ramp, sunAltitude } from "../daylight";
  import { knob, loadPulse, onAction, pickRange, pulse, setKnob, takeRange } from "../overview.svelte";
  import { prefs } from "../prefs.svelte";
  import { snap, snapDown } from "../pixels.svelte";
  import { TEXT_MIN } from "../textSize";
  import { poll } from "../poll";
  import HowCounted from "./HowCounted.svelte";

  const RANGES = [30, 90, 371];
  const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
  const LONG = ["Mondays", "Tuesdays", "Wednesdays", "Thursdays", "Fridays", "Saturdays", "Sundays"];
  const DAY = 86_400_000;

  const days = $derived.by(() => {
    void prefs.overviewFrames;
    return Number(knob<string>("clock", "range"));
  });
  /** The end of the window: now, except while the year plays. */
  let at = $state(Date.now());
  let playing = $state(false);
  let halt: (() => void) | undefined;
  const daylight = $derived.by(() => {
    void prefs.overviewFrames;
    void prefs.overviewDaylight;
    return knob<boolean>("clock", "daylight");
  });
  loadPulse();
  onAction("clock", "play", () => {
    if (!daylight) setKnob("clock", "daylight", true);
    if (!playing) play();
  });

  /** Runs the window from a year ago to now, a few days a tick, and stops at today. */
  function play() {
    if (playing) {
      stop();
      return;
    }
    const end = Date.now();
    let t = end - 365 * DAY;
    playing = true;
    halt = poll(() => {
      t += 3 * DAY;
      if (t >= end) stop();
      else at = t;
    }, 90);
  }
  function stop() {
    halt?.();
    halt = undefined;
    playing = false;
    at = Date.now();
  }
  $effect(() => () => halt?.());

  // A range picked in a linked card moves this one to its nearest range (overview.svelte.ts).
  let handed = false;
  takeRange("clock", RANGES, (d) => {
    handed = true;
    setKnob("clock", "range", String(d));
  });
  // A period picked in the menu reaches the cards linked to this one; one handed by them does not go back.
  let last: number | null = null;
  $effect(() => {
    if (last !== null && days !== last && !handed) pickRange("clock", days);
    handed = false;
    last = days;
  });

  /** 168 counts: Monday 00 first, Sunday 23 last. */
  const counts = $derived.by(() => {
    const out = new Array(168).fill(0);
    const since = at - days * DAY;
    for (const ts of pulse.data?.moments ?? []) {
      if (ts < since || ts > at) continue;
      const d = new Date(ts);
      out[((d.getDay() + 6) % 7) * 24 + d.getHours()]++;
    }
    return out;
  });
  const total = $derived(counts.reduce((a, b) => a + b, 0));

  /** How light each hour of the day at the end of the window is, at your place: 24 values. */
  const lit = $derived(daylight ? daySlice(at, prefs.place) : null);
  /** Share of your writing done while it was light, by the sun at each message's own moment
   *  (the twilight counts for how light it was). */
  const inLight = $derived.by(() => {
    if (!lit || !total) return 0;
    const since = at - days * DAY;
    let sum = 0;
    for (const ts of pulse.data?.moments ?? []) if (ts >= since && ts <= at) sum += light(sunAltitude(ts, prefs.place));
    return Math.round((sum / total) * 100);
  });
  /** The cell this hour of this weekday is in. */
  const here = $derived.by(() => {
    const d = new Date(at);
    return ((d.getDay() + 6) % 7) * 24 + d.getHours();
  });

  /** Four steps at the quartiles of the hours that had anything. */
  const steps = $derived.by(() => {
    const had = counts.filter((c) => c > 0).sort((a, b) => a - b);
    const q = (f: number) => had[Math.floor((had.length - 1) * f)] ?? 0;
    return [q(0.25), q(0.5), q(0.75)];
  });
  const level = (n: number) => (!n ? 0 : n <= steps[0] ? 1 : n <= steps[1] ? 2 : n <= steps[2] ? 3 : 4);

  /** The three hours in a row, across midnight too, where you wrote the most. */
  const peak = $derived.by(() => {
    let best = -1;
    let start = 0;
    for (let i = 0; i < 168; i++) {
      const sum = counts[i] + counts[(i + 1) % 168] + counts[(i + 2) % 168];
      if (sum > best) [best, start] = [sum, i];
    }
    if (best <= 0) return null;
    const hour = (h: number) => String(h % 24).padStart(2, "0");
    return { day: LONG[Math.floor(start / 24)], from: hour(start % 24), to: hour((start % 24) + 3) };
  });

  /** Share of your writing between midnight and five. */
  const night = $derived.by(() => {
    if (!total) return 0;
    let n = 0;
    for (let d = 0; d < 7; d++) for (let h = 0; h < 5; h++) n += counts[d * 24 + h];
    return Math.round((n / total) * 100);
  });

  const tip = (d: number, h: number) => {
    const n = counts[d * 24 + h];
    const l = lit ? ` · ${Math.round(lit[h] * 100)}% daylight today` : "";
    return `${LONG[d]} ${String(h).padStart(2, "0")}–${String((h + 1) % 24).padStart(2, "0")}: ${n} ${n === 1 ? "message" : "messages"}${l}`;
  };
  const dm = (ms: number) => new Date(ms).toLocaleDateString([], { day: "numeric", month: "short" });

  // The cells, the gaps and the label column are whole device pixels (pixels.svelte.ts), worked out
  // from the card's width, so they line up and the gaps stay even at any zoom: at 125% a 2px gap
  // is 2.5 device pixels, which showed as some gaps wider and darker than others.
  let width = $state(0);
  const gap = $derived(snap(2));
  // Room for "Wed" in the weekday labels' font: 11px grown by the text size (vite.config.ts).
  const label = $derived(snap(2.8 * Math.max(10, (TEXT_MIN[prefs.textSize] ?? 0) - 2.5)));
  const cell = $derived(width ? Math.max(snap(8), snapDown((width - label - 23 * gap) / 24)) : 14);
</script>

<HowCounted card="clock">
  <p>One mark per message you sent, by this computer's clock.</p>
</HowCounted>

{#if !pulse.data && !pulse.loaded}
  <Quiet kind="loading" />
{:else if !total}
  <p class="quiet">{pulse.error ? `Could not read: ${pulse.error}` : "No messages or prompts in this range."}</p>
{:else}
  <div role="presentation" class="clock" class:playing style:--cell="{cell}px" style:--gap="{gap}px" style:--label="{label}px" bind:clientWidth={width} onclick={() => playing && stop()}>
    <span></span>
    {#each { length: 24 } as _, h (h)}<span class="hour">{h % 6 === 0 ? String(h).padStart(2, "0") : ""}</span>{/each}
    {#each DAYS as name, d (name)}
      <span class="day">{name}</span>
      {#each { length: 24 } as _, h (h)}
        {@const i = d * 24 + h}
        <span class="cell l{level(counts[i])}" class:sol={lit} class:now={lit && !playing && i === here} style={lit ? `--c:${ramp(lit[h])}` : ""} data-tip={tip(d, h)}></span>
      {/each}
    {/each}
  </div>
  {#if playing}
    <p class="caption">{dm(at - days * DAY)} – {dm(at)} · click to stop</p>
  {:else if peak}
    <p class="caption">Busiest <em>{peak.day} {peak.from}–{peak.to}</em>{#if lit}{" · "}{inLight}% in daylight{:else if night}{" · "}{night}% after midnight{/if}{" · "}{days === 371 ? "a year" : days === 30 ? "a month" : `${days} days`}</p>
  {/if}
{/if}

<style>
  .clock { display: grid; grid-template-columns: var(--label, 28px) repeat(24, var(--cell)); gap: var(--gap, 2px); }
  .clock.playing { cursor: pointer; }
  .hour, .day { font-size: 11px; color: var(--muted); line-height: 1; }
  .hour { height: 12px; width: 0; white-space: nowrap; overflow: visible; }
  .day { align-self: center; }
  .cell { width: var(--cell); height: var(--cell); border-radius: 2px; background: var(--surface-hover); }
  .l1 { background: color-mix(in srgb, var(--accent) 24%, transparent); }
  .l2 { background: color-mix(in srgb, var(--accent) 45%, transparent); }
  .l3 { background: color-mix(in srgb, var(--accent) 70%, transparent); }
  .l4 { background: var(--accent); }
  /* Daylight: the hour's colour, and the count as how solid it is. */
  .cell.sol.l1 { background: rgba(var(--c), 0.3); }
  .cell.sol.l2 { background: rgba(var(--c), 0.52); }
  .cell.sol.l3 { background: rgba(var(--c), 0.78); }
  .cell.sol.l4 { background: rgb(var(--c)); }
  /* This hour today: a ring with a dot in it, so it shows among the lit cells too. */
  .cell.now { outline: 1.5px solid var(--text); outline-offset: 1px; position: relative; z-index: 1; }
  .cell.now::after { content: ""; position: absolute; inset: 50% auto auto 50%; width: 4px; height: 4px; margin: -2px 0 0 -2px; border-radius: 50%; background: var(--text); box-shadow: 0 0 0 1.5px var(--surface); }
</style>
