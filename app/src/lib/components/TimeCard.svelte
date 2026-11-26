<!-- Clock: the time, in one of four shapes picked in the card's menu (the Shape knob), all in the day's light at your place (violet through the night, orange at dawn and dusk, gold in the day). Ring: the time in the middle of a ring that is the day, midnight at the bottom and noon at the top, with notches for sunrise and sunset, a flat sun riding it that turns into a moon through the twilight, a short trail, stars in the dark, and the seconds as a thin arc. Boxes: the day as 24 squares, six to a row, the hour you are in ringed, the minutes as twelve squares filling up. Grids: hours, minutes and seconds as three grids filling in. Rails: the same as three rows. Date and week under each.
     Used by CardBody.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  // Clock: the time, in one of four shapes picked in the card's menu (the Shape knob), all in
  // the day's light at your place (violet through the night, orange at dawn and dusk, gold in the
  // day). Ring: the time in the middle of a ring that is the day, midnight at the bottom and noon
  // at the top, with notches for sunrise and sunset, a flat sun riding it that turns into a moon
  // through the twilight, a short trail, stars in the dark, and the seconds as a thin arc. Boxes:
  // the day as 24 squares, six to a row, the hour you are in ringed, the minutes as twelve squares
  // filling up. Grids: hours, minutes and seconds as three grids filling in. Rails: the same as
  // three rows. Date and week under each.
  import { dayRing, daySlice, light, ramp, sunAltitude, sunTimes } from "../daylight";
  import { knob } from "../overview.svelte";
  import { prefs, type ClockKind } from "../prefs.svelte";
  import HowCounted from "./HowCounted.svelte";

  const kind = $derived.by(() => {
    void prefs.overviewFrames;
    void prefs.clockKind;
    return knob<ClockKind>("time", "shape");
  });

  let now = $state(Date.now());
  $effect(() => {
    const t = poll(() => (now = Date.now()), 1000);
    return () => t();
  });
  const place = $derived(prefs.place);
  const d = $derived(new Date(now));
  const hh = $derived(String(d.getHours()).padStart(2, "0"));
  const mm = $derived(String(d.getMinutes()).padStart(2, "0"));
  const ss = $derived(String(d.getSeconds()).padStart(2, "0"));
  const date = $derived(d.toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" }));
  /** ISO week. */
  const week = $derived.by(() => {
    const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
    t.setUTCDate(t.getUTCDate() + 4 - (t.getUTCDay() || 7));
    return Math.ceil(((+t - +new Date(Date.UTC(t.getUTCFullYear(), 0, 1))) / 86_400_000 + 1) / 7);
  });

  /** Noon today: the day's ring and sun times only change when the day turns. */
  const today = $derived.by(() => {
    const t = new Date(now);
    t.setHours(12, 0, 0, 0);
    return t.getTime();
  });
  /** 96 quarter-hours of light, midnight first. */
  const ring = $derived(dayRing(today, 96, place));
  /** 24 hours of light, for the squares. */
  const slice = $derived(daySlice(today, place));
  const sun = $derived(sunTimes(today, place));
  const hourNow = $derived(d.getHours());
  const minNow = $derived(d.getMinutes());
  const secNow = $derived(d.getSeconds());

  const mins = $derived(d.getHours() * 60 + d.getMinutes() + d.getSeconds() / 60);
  /** Degrees around the ring: 0 at the bottom (midnight), clockwise. */
  const ang = $derived((mins / 1440) * 360);
  const secA = $derived((d.getSeconds() / 60) * 360);
  /** A point on a circle of radius r at `deg` from the bottom, clockwise, in the 120-unit box. */
  const P = (deg: number, r: number) => [60 - r * Math.sin((deg * Math.PI) / 180), 60 + r * Math.cos((deg * Math.PI) / 180)];
  /** An arc from `a` to `b` degrees (clockwise from the bottom) at radius r. */
  const seg = (a: number, b: number, r: number) => {
    const [x1, y1] = P(a, r), [x2, y2] = P(b, r);
    return `M${x1.toFixed(2)} ${y1.toFixed(2)} A${r} ${r} 0 ${b - a > 180 ? 1 : 0} 1 ${x2.toFixed(2)} ${y2.toFixed(2)}`;
  };
  const smooth = (a: number, b: number, x: number) => {
    const t = Math.min(1, Math.max(0, (x - a) / (b - a)));
    return t * t * (3 - 2 * t);
  };
  /** 0 with the sun up, 1 in the dark, through in the twilight: how much of a moon the marker is. */
  const moon = $derived(1 - smooth(0.25, 0.75, light(sunAltitude(now, place))));
  /** The marker's colour: a shade lighter than the ring, so the sun still shows over gold at noon. */
  const mc = $derived([252, 234, 190].map((v, i) => Math.round(v + ([206, 209, 250][i] - v) * moon)).join(","));
  /** The trail's colour: the ring's own gold or violet. */
  const tc = $derived([241, 207, 142].map((v, i) => Math.round(v + ([186, 190, 245][i] - v) * moon)).join(","));
  const mark = $derived(P(ang, 46));
  const STARS = [[38, 44], [78, 36], [84, 70], [44, 80], [62, 30]];
  const RAYS = [0, 45, 90, 135, 180, 225, 270, 315];
</script>

<HowCounted card="time">
  <p>This computer's clock, in the day's light.</p>
</HowCounted>
  {#if kind === "boxes"}
    <div class="boxes">
      <div class="lab">
        {#each [0, 6, 12, 18] as r (r)}<span>{String(r).padStart(2, "0")}</span>{/each}
        <div class="hours">{#each slice as l, h (h)}<i class:now={h === hourNow} style="background: rgba({ramp(l)}, {(0.18 + 0.82 * l).toFixed(2)})" data-tip="{String(h).padStart(2, '0')}:00"></i>{/each}</div>
      </div>
      <div class="beside">
        <span class="big">{hh}:{mm}</span><span class="sec">{ss}</span>
        <div class="mins">{#each { length: 12 } as _, i (i)}<i class:on={i < Math.floor(minNow / 5)} class:part={i === Math.floor(minNow / 5)} style="--f:{((minNow % 5) / 5).toFixed(2)};--m:{tc}"></i>{/each}</div>
        <p class="date left">{date} · week {week}</p>
      </div>
    </div>
  {:else if kind === "grids"}
    <div class="grids">
      <div class="grp"><span class="cap">{hh}</span>
        <div class="lab">
          {#each [0, 6, 12, 18] as r (r)}<span>{String(r).padStart(2, "0")}</span>{/each}
          <div class="hours">{#each slice as l, h (h)}<i class:now={h === hourNow} style="background: rgba({ramp(l)}, {(0.18 + 0.82 * l).toFixed(2)})"></i>{/each}</div>
        </div>
      </div>
      <div class="grp"><span class="cap">{mm}</span><div class="g60">{#each { length: 60 } as _, i (i)}<i class:on={i < minNow} class:now={i === minNow} style="--m:{tc}"></i>{/each}</div></div>
      <div class="grp"><span class="cap">{ss}</span><div class="g60 s">{#each { length: 60 } as _, i (i)}<i class:on={i < secNow} class:now={i === secNow} style="--m:{tc}"></i>{/each}</div></div>
    </div>
    <p class="date left">{date} · week {week}</p>
  {:else if kind === "rails"}
    <div class="rails">
      <div class="rail"><span class="cap">{hh}</span><div class="r24">{#each slice as l, h (h)}<i class:now={h === hourNow} style="background: rgba({ramp(l)}, {(0.18 + 0.82 * l).toFixed(2)})"></i>{/each}</div></div>
      <div class="rail"><span class="cap">{mm}</span><div class="r60">{#each { length: 60 } as _, i (i)}<i class:on={i < minNow} class:now={i === minNow} style="--m:{tc}"></i>{/each}</div></div>
      <div class="rail"><span class="cap">{ss}</span><div class="r60">{#each { length: 60 } as _, i (i)}<i class:on={i < secNow} class:now={i === secNow} style="--m:{tc}"></i>{/each}</div></div>
    </div>
    <p class="date left">{date} · week {week}</p>
  {:else}
  <div class="face">
    <svg viewBox="0 0 120 120" overflow="visible" aria-label="{hh}:{mm}:{ss}">
      <defs>
        <mask id="crescent"><rect x="-14" y="-14" width="28" height="28" fill="#fff" /><circle cx={14 - 10.4 * moon} cy={-2.2 * moon} r="5.6" fill="#000" /></mask>
      </defs>
      <!-- 24 hour ticks on the inner edge, the quarters a little longer -->
      {#each { length: 24 } as _, h (h)}
        <line x1={P(h * 15, 37)[0]} y1={P(h * 15, 37)[1]} x2={P(h * 15, h % 6 ? 35.6 : 34)[0]} y2={P(h * 15, h % 6 ? 35.6 : 34)[1]} stroke="var(--muted)" stroke-opacity={h % 6 ? 0.35 : 0.8} stroke-width={h % 6 ? 0.7 : 1} stroke-linecap="round" />
      {/each}
      <!-- the day as a ring of light: 96 quarter-hours, each a little over its share so no seams show -->
      <g stroke-width="8" fill="none">
        {#each ring as l, i (i)}<path d={seg(i * 3.75, i * 3.75 + 4.2, 46)} stroke="rgb({ramp(l)})" stroke-opacity={0.14 + 0.86 * l} />{/each}
      </g>
      <!-- thin rims give the ring an edge -->
      <circle cx="60" cy="60" r="41.6" fill="none" stroke="var(--surface)" stroke-width="1.2" />
      <circle cx="60" cy="60" r="50.4" fill="none" stroke="var(--surface)" stroke-width="1.2" />
      <!-- sunrise and sunset: notches outside the ring -->
      {#if sun}
        {#each [sun.rise, sun.set] as m (m)}
          <path d={seg((m / 1440) * 360 - 0.9, (m / 1440) * 360 + 0.9, 53)} stroke="rgb(241,207,142)" stroke-width="3.2" stroke-linecap="round" fill="none" opacity="0.9" />
        {/each}
      {/if}
      <!-- the trail behind the marker -->
      {#each [0, 1, 2, 3, 4, 5] as k (k)}
        <path d={seg(ang - 4 - k * 4, ang - k * 4, 46)} stroke="rgb({tc})" stroke-width="8" fill="none" opacity={(0.28 * (1 - k / 6)).toFixed(2)} />
      {/each}
      <!-- stars come out in the dark -->
      {#each STARS as [x, y], i (i)}<circle cx={x} cy={y} r={i % 2 ? 0.7 : 0.9} fill="#dfe3ff" opacity={(moon * (0.55 - i * 0.06)).toFixed(2)} />{/each}
      <!-- the seconds as a thin arc from the top -->
      <path d={seg(180.01, 180 + secA, 39.6)} stroke="rgb({tc})" stroke-width="0.8" fill="none" stroke-linecap="round" opacity="0.45" />
      <circle cx={P(180 + secA, 39.6)[0]} cy={P(180 + secA, 39.6)[1]} r="1.1" fill="rgb({tc})" opacity="0.9" />
      <!-- the marker: a flat sun by day, a crescent moon by night, one shade lighter than the ring -->
      <g transform="translate({mark[0].toFixed(2)} {mark[1].toFixed(2)})">
        <g opacity={((1 - moon) * (1 - moon)).toFixed(2)}>
          {#each RAYS as a (a)}
            <line x1={Math.cos((a * Math.PI) / 180) * 6.4} y1={Math.sin((a * Math.PI) / 180) * 6.4} x2={Math.cos((a * Math.PI) / 180) * 7.8} y2={Math.sin((a * Math.PI) / 180) * 7.8} stroke="rgb({mc})" stroke-width="1.2" stroke-linecap="round" />
          {/each}
        </g>
        <circle r="5" fill="rgb({mc})" mask="url(#crescent)" />
      </g>
      <text x="60" y="58" text-anchor="middle" class="t">{hh}:{mm}</text>
      <text x="60" y="71" text-anchor="middle" class="s">{ss}</text>
    </svg>
    <p class="date">{date} · week {week}</p>
  </div>
{/if}

<style>
  /* The ring is as big as the card lets it: half its width, between a small and a large face. */
  .face { display: grid; place-items: center; }
  /* The digits are drawn in the ring's own units, so the ring grows with the text instead. */
  svg { overflow: visible; width: clamp(calc(150px * var(--text-grow)), 50cqw, calc(260px * var(--text-grow))); height: auto; }
  .t { --keep-size: 1; font-size: 21px; font-weight: 500; fill: var(--text); letter-spacing: -0.02em; font-variant-numeric: tabular-nums; font-family: var(--font); }
  .s { --keep-size: 1; font-size: 8.5px; fill: var(--muted); font-variant-numeric: tabular-nums; font-family: var(--font); letter-spacing: 0.08em; }
  .date { margin: 6px 0 0; text-align: center; color: var(--muted); font-size: 12px; letter-spacing: 0.01em; }
  .date.left { text-align: left; margin-top: 10px; }
  /* Boxes: the day as squares with 00 06 12 18 at the rows, the time and the minutes beside. */
  .boxes { display: flex; gap: 18px; align-items: center; flex-wrap: wrap; }
  .lab { display: grid; grid-template-columns: 16px auto; grid-auto-rows: 16px; column-gap: 6px; row-gap: 3px; align-items: center; }
  .lab span { --keep-size: 1; font-size: 10px; color: var(--muted); line-height: 1; text-align: right; }
  .lab .hours { grid-column: 2; grid-row: 1 / span 4; }
  .hours { display: grid; grid-template-columns: repeat(6, 16px); grid-auto-rows: 16px; gap: 3px; flex: none; }
  .hours i { border-radius: 3px; }
  .hours i.now, .g60 i.now, .r24 i.now, .r60 i.now { outline: 1.5px solid var(--text); outline-offset: 1px; }
  .beside { min-width: 0; }
  .big { font-size: 34px; font-weight: 500; letter-spacing: -0.02em; color: var(--text); font-variant-numeric: tabular-nums; line-height: 1; }
  .sec { margin-left: 6px; font-size: 16px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .mins { display: flex; gap: 3px; margin-top: 8px; }
  .mins i { width: 10px; height: 10px; border-radius: 2px; background: var(--surface-hover); }
  .mins i.on { background: rgb(var(--m)); }
  .mins i.part { background: linear-gradient(90deg, rgb(var(--m)) calc(var(--f) * 100%), var(--surface-hover) 0); }
  /* Grids: hours 6 by 4, minutes and seconds 12 by 5. */
  .grids { display: flex; gap: 22px; align-items: flex-start; flex-wrap: wrap; }
  .grp { display: grid; gap: 6px; }
  .cap { font-size: 22px; font-weight: 500; color: var(--text); font-variant-numeric: tabular-nums; line-height: 1; }
  .g60 { display: grid; grid-template-columns: repeat(12, 9px); grid-auto-rows: 9px; gap: 3px; }
  .g60.s { grid-template-columns: repeat(12, 7px); grid-auto-rows: 7px; }
  .g60 i { border-radius: 2px; background: var(--surface-hover); transition: background var(--dur-move) var(--ease); }
  .g60 i.on { background: rgba(var(--m), 0.85); }
  .g60 i.now { background: rgb(var(--m)); }
  .g60.s i.on { opacity: 0.7; }
  /* Rails: hours, minutes and seconds as three rows across the card. */
  .rails { display: grid; gap: 10px; }
  .rail { display: grid; grid-template-columns: 34px 1fr; align-items: center; gap: 10px; }
  .rail .cap { font-size: 18px; text-align: right; }
  .r24 { display: grid; grid-template-columns: repeat(24, 1fr); gap: 3px; }
  .r24 i { aspect-ratio: 1; border-radius: 2px; }
  .r60 { display: grid; grid-template-columns: repeat(60, 1fr); gap: 2px; }
  .r60 i { aspect-ratio: 1; border-radius: 1.5px; background: var(--surface-hover); }
  .r60 i.on { background: rgba(var(--m), 0.85); }
  .r60 i.now { background: rgb(var(--m)); }
</style>
