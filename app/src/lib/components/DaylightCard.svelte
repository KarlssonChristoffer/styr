<!-- Daylight: the day at your place as a picture. A horizon coloured by how light each hour is (violet, orange, gold), the sun's arc over it from sunrise to sunset with the times at its feet, yesterday's arc as a dotted line behind it, the night's path under the horizon, and where the sun is right now: a flat sun on the arc by day, a crescent moon under it by night. One line under it says the hours of sun today and how that differs from yesterday (never red and green: a shorter day is not an error).
     Used by CardBody.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  // Daylight: the day at your place as a picture. A horizon coloured by how light each hour is
  // (violet, orange, gold), the sun's arc over it from sunrise to sunset with the times at its
  // feet, yesterday's arc as a dotted line behind it, the night's path under the horizon, and
  // where the sun is right now: a flat sun on the arc by day, a crescent moon under it by night.
  // One line under it says the hours of sun today and how that differs from yesterday (never red
  // and green: a shorter day is not an error).
  import { dayLength, daySlice, duration, hhmm, light, ramp, sunAltitude, sunTimes, GOLD } from "../daylight";
  import { prefs } from "../prefs.svelte";
  import HowCounted from "./HowCounted.svelte";

  const DAY = 86_400_000;
  let now = $state(Date.now());
  // Once a minute is enough: the marker moves a pixel or so.
  $effect(() => {
    const t = poll(() => (now = Date.now()), 60_000);
    return () => t();
  });
  /** Noon today, so the day's numbers only recompute when the day turns. */
  const today = $derived.by(() => {
    const d = new Date(now);
    d.setHours(12, 0, 0, 0);
    return d.getTime();
  });
  const place = $derived(prefs.place);
  const sun = $derived(sunTimes(today, place));
  const before = $derived(sunTimes(today - DAY, place));
  const len = $derived(dayLength(today, place));
  const delta = $derived(len - dayLength(today - DAY, place));
  const slice = $derived(daySlice(today, place));
  /** Minutes into today, now. */
  const minutes = $derived.by(() => {
    const d = new Date(now);
    return d.getHours() * 60 + d.getMinutes();
  });
  const up = $derived(sunAltitude(now, place) > -0.833);

  // The picture: 600 wide, the horizon at HZ, a day arc up to TOP and the night's path down to LOW.
  const W = 600, H = 120, HZ = 86, TOP = 10, LOW = 112;
  const x = (m: number) => (m / 1440) * W;
  /** The sun's path from `a` to `b` minutes as a curve that peaks (or dips) at `apex`. */
  const curve = (a: number, b: number, apex: number) => `M${x(a).toFixed(1)},${HZ} Q${x((a + b) / 2).toFixed(1)},${apex} ${x(b).toFixed(1)},${HZ}`;
  /** Where on that curve the moment `m` is, as a point. */
  function on(a: number, b: number, apex: number, m: number): [number, number] {
    const t = Math.min(1, Math.max(0, (m - a) / (b - a)));
    const y = (1 - t) * (1 - t) * HZ + 2 * (1 - t) * t * apex + t * t * HZ;
    return [x(a) + t * (x(b) - x(a)), y];
  }
  /** Today's day as rise and set minutes: the sun's times, a whole day in polar day, none in polar night. */
  const span = $derived(sun ?? (len ? { rise: 0, set: 1440 } : null));
  const yspan = $derived(before ?? (len - delta ? { rise: 0, set: 1440 } : null));
  /** The night as one path from sunset to the next sunrise, drawn twice a day apart so both ends show. */
  const night = $derived(span ? { a: span.set, b: span.rise + 1440 } : { a: 0, b: 1440 });
  /** The marker's place: on the day's arc with the sun up, else on the night's path. */
  const mark = $derived.by((): [number, number] => {
    if (span && up) return on(span.rise, span.set, TOP, minutes);
    const m = minutes < night.a ? minutes + 1440 : minutes;
    const [px, py] = on(night.a, night.b, LOW, m);
    return [px > W ? px - W : px, py];
  });
  const moon = $derived(1 - Math.min(1, Math.max(0, (light(sunAltitude(now, place)) - 0.25) / 0.5)));
  const stops = $derived(slice.map((l, h) => `<stop offset="${((h + 0.5) / 24 * 100).toFixed(1)}%" stop-color="rgb(${ramp(l)})"/>`).join(""));
  const RAYS = [0, 45, 90, 135, 180, 225, 270, 315];
</script>

<HowCounted card="daylight">
  <p>Hours of sun at your place, worked out from the date.</p>
</HowCounted>

<svg class="sky" viewBox="0 0 {W} {H}" aria-label="{len ? duration(len) : 'No sun'} of daylight today">
  <defs>
    <linearGradient id="dl-horizon" x1="0" x2="1">{@html stops}</linearGradient>
    <linearGradient id="dl-fade" x1="0" x2="0" y1="0" y2="1"><stop offset="0" stop-color="rgba({GOLD},0.3)" /><stop offset="1" stop-color="rgba({GOLD},0.02)" /></linearGradient>
    <mask id="dl-crescent"><rect x="-14" y="-14" width="28" height="28" fill="#fff" /><circle cx={14 - 10.4 * moon} cy={-2.2 * moon} r="5.6" fill="#000" /></mask>
  </defs>
  {#if span}
    <path d="{curve(span.rise, span.set, TOP)} Z" fill="url(#dl-fade)" />
  {/if}
  {#if yspan}
    <path d={curve(yspan.rise, yspan.set, TOP + 4)} fill="none" stroke="var(--line-strong)" stroke-width="1" stroke-dasharray="2 3" />
  {/if}
  <path d={curve(night.a, night.b, LOW)} fill="none" stroke="rgba(106,111,208,0.55)" stroke-width="1.2" />
  <path d={curve(night.a - 1440, night.b - 1440, LOW)} fill="none" stroke="rgba(106,111,208,0.55)" stroke-width="1.2" />
  {#if span}
    <path d={curve(span.rise, span.set, TOP)} fill="none" stroke="rgb({GOLD})" stroke-width="1.6" />
  {/if}
  <rect x="0" y={HZ - 1.5} width={W} height="3" rx="1.5" fill="url(#dl-horizon)" />
  {#each [0, 6, 12, 18, 24] as h (h)}
    <text x={x(h * 60)} y={H - 2} class="t" text-anchor={h === 0 ? "start" : h === 24 ? "end" : "middle"}>{String(h % 24).padStart(2, "0")}</text>
  {/each}
  {#if sun}
    <text x={x(sun.rise)} y={HZ + 15} class="t gold" text-anchor="middle">{hhmm(sun.rise)}</text>
    <text x={x(sun.set)} y={HZ + 15} class="t gold" text-anchor="middle">{hhmm(sun.set)}</text>
  {/if}
  <!-- now: a flat sun on the arc by day, a crescent moon under the horizon by night -->
  <g transform="translate({mark[0].toFixed(1)} {mark[1].toFixed(1)})">
    <g opacity={((1 - moon) * (1 - moon)).toFixed(2)}>
      {#each RAYS as a (a)}
        <line x1={Math.cos((a * Math.PI) / 180) * 7.2} y1={Math.sin((a * Math.PI) / 180) * 7.2} x2={Math.cos((a * Math.PI) / 180) * 9} y2={Math.sin((a * Math.PI) / 180) * 9} stroke="rgb(252,234,190)" stroke-width="1.3" stroke-linecap="round" />
      {/each}
    </g>
    <circle r="5.4" fill={moon > 0.5 ? "#c9cbea" : "rgb(252,234,190)"} mask="url(#dl-crescent)" />
  </g>
</svg>
<p class="caption big">
  <span class="figure">{len ? duration(len) : "No sun"}</span>
  <span>{len ? "of daylight today" : "today"}</span>
  <em class="gold">{delta < 0 ? "−" : delta > 0 ? "+" : "±"}{duration(Math.abs(delta))} since yesterday</em>
</p>

<style>
  .sky { display: block; width: 100%; height: auto; overflow: hidden; }
  .t { --keep-size: 1; font-size: 10px; fill: var(--muted); font-family: var(--font); }
  .t.gold { fill: var(--daylight-gold, rgb(241, 207, 142)); }
  .caption.big { display: flex; align-items: baseline; gap: 8px; margin-top: 4px; }
  .caption.big .gold { margin-left: auto; color: var(--daylight-gold, rgb(241, 207, 142)); font-weight: 400; font-variant-numeric: tabular-nums; }
</style>
