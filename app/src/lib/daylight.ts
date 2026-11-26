// Where the sun is, at your place, for the Overview's daylight colours: the When you work map
// (how light each hour of today is), the Daylight card (today's sun hours against
// yesterday's) and the Clock card (the day as a ring of light). Plain astronomy, no weather and
// no library: the sun's position from the date (the NOAA approximation, good to a minute or so),
// which is all a colour needs. Everything is in this computer's clock and `place` is a setting
// (`prefs.place`, Stockholm until you change it).

export type Place = { name: string; lat: number; lon: number };

const RAD = Math.PI / 180;
const DAY = 86_400_000;

/** The sun's height in degrees above the horizon at `ms`, seen from `place`. */
export function sunAltitude(ms: number, place: Place): number {
  const n = ms / DAY + 2440587.5 - 2451545.0; // days since J2000
  const L = (280.46 + 0.9856474 * n) % 360; // mean longitude
  const g = ((357.528 + 0.9856003 * n) % 360) * RAD; // mean anomaly
  const lam = (L + 1.915 * Math.sin(g) + 0.02 * Math.sin(2 * g)) * RAD; // ecliptic longitude
  const eps = 23.439 * RAD;
  const dec = Math.asin(Math.sin(eps) * Math.sin(lam));
  const ra = Math.atan2(Math.cos(eps) * Math.sin(lam), Math.cos(lam));
  const gmst = (18.697374558 + 24.06570982441908 * n) % 24;
  const lst = ((((gmst + place.lon / 15) % 24) + 24) % 24) * 15 * RAD;
  const ha = lst - ra;
  const la = place.lat * RAD;
  return Math.asin(Math.sin(la) * Math.sin(dec) + Math.cos(la) * Math.cos(dec) * Math.cos(ha)) / RAD;
}

/** How light it is for a sun height: 0 in the dark (six degrees under the horizon or more, the
 *  end of civil twilight), 1 in full day (six above), smooth between for dawn and dusk. */
export const light = (alt: number) => Math.min(1, Math.max(0, (alt + 6) / 12));

/** Midnight at the start of the day `ms` is in, in this computer's clock. */
function startOfDay(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/** How light an hour of a day is on average: six samples, ten minutes apart. */
function hourLight(day: number, h: number, place: Place): number {
  let s = 0;
  for (let m = 5; m < 60; m += 10) {
    const t = new Date(day);
    t.setHours(h, m, 0, 0);
    s += light(sunAltitude(t.getTime(), place));
  }
  return s / 6;
}

/** 24 values: how light each hour of the day `ms` is in. */
export function daySlice(ms: number, place: Place): number[] {
  const day = startOfDay(ms);
  return Array.from({ length: 24 }, (_, h) => hourLight(day, h, place));
}

/** `n` samples through the day `ms` is in, midnight first: how light it is at each. The ring
 *  clock draws 96, one a quarter of an hour. */
export function dayRing(ms: number, n: number, place: Place): number[] {
  const day = startOfDay(ms);
  return Array.from({ length: n }, (_, i) => light(sunAltitude(day + (i * DAY) / n, place)));
}

export type SunTimes = { rise: number; set: number };

/** Sunrise and sunset on the day `ms` is in, as minutes after midnight (the sun's upper edge at
 *  the horizon, to two minutes), or null in polar day or polar night. */
export function sunTimes(ms: number, place: Place): SunTimes | null {
  const day = startOfDay(ms);
  let rise = -1;
  let set = -1;
  for (let m = 0; m < 1440; m += 2) {
    const t = new Date(day);
    t.setMinutes(m);
    if (sunAltitude(t.getTime(), place) > -0.833) {
      if (rise < 0) rise = m;
      set = m;
    }
  }
  return rise < 0 || (rise === 0 && set >= 1438) ? null : { rise, set };
}

/** Minutes of sun on the day `ms` is in: 0 in polar night, a whole day in polar day. */
export function dayLength(ms: number, place: Place): number {
  const s = sunTimes(ms, place);
  if (s) return s.set - s.rise;
  return sunAltitude(startOfDay(ms) + DAY / 2, place) > 0 ? 1440 : 0;
}

/** Minutes after midnight as "07:08". */
export const hhmm = (m: number) => `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;

/** Minutes as "8 h 46 min", or "46 min" under an hour. */
export const duration = (m: number) => (m >= 60 ? `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, "0")} min` : `${m} min`);

/** The daylight colours: violet in the dark, dusk orange in the twilight, gold in the day. */
const STOPS: [number, [number, number, number]][] = [
  [0, [106, 111, 208]],
  [0.5, [233, 147, 107]],
  [1, [241, 207, 142]],
];

/** The colour for how light it is (0 to 1), as "r,g,b" for `rgb()` and `rgba()`. */
export function ramp(l: number): string {
  const [a, b] = l < 0.5 ? [STOPS[0], STOPS[1]] : [STOPS[1], STOPS[2]];
  const f = (Math.min(1, Math.max(0, l)) - a[0]) / (b[0] - a[0]);
  return a[1].map((v, i) => Math.round(v + (b[1][i] - v) * f)).join(",");
}

/** Gold, the colour of the sun and of what daylight says. */
export const GOLD = "241,207,142";
