// Scenes: what Home (and the page above the message box on New thread) shows. Hub picks the one
// that fits what is going on, so there is nothing to lay out; the tabs on top let you look at
// another. The pick is a pure function of a few counts, so it only changes when the state does.

export type SceneId = "needs" | "live" | "away" | "projects" | "today";

export const SCENES: { id: SceneId; name: string }[] = [
  { id: "needs", name: "Needs you" },
  { id: "live", name: "Live" },
  { id: "away", name: "While you were away" },
  { id: "projects", name: "Projects" },
  { id: "today", name: "Today" },
];

/** What decides the scene. */
export type Facts = {
  /** Threads that wait on you (a permission, a failure, a reply that asks). */
  needs: number;
  /** Threads that are working. */
  running: number;
  /** You came back after a while and something changed meanwhile. */
  away: boolean;
  /** Hour of the day, 0 to 23. */
  hour: number;
  /** Threads that moved today. */
  today: number;
};

/** Evening is when the day is worth looking back on. */
export const EVENING = 17;

/** The scene that fits now: Live when something waits on you (it lists those first, under the graphs),
 *  then what happened while you were gone, then what runs, the day in the evening, and your projects
 *  when it is quiet. The Needs you scene is one tab away. */
export function pickScene(f: Facts): SceneId {
  if (f.needs > 0) return "live";
  if (f.away) return "away";
  if (f.running > 0) return "live";
  if (f.hour >= EVENING && f.today > 0) return "today";
  return "projects";
}

/** Tabs for scenes with nothing to show now are dimmed, not hidden: they can still be opened. */
export function dimmed(f: Facts): SceneId[] {
  const out: SceneId[] = [];
  if (!f.needs) out.push("needs");
  if (!f.running && !f.needs) out.push("live");
  if (!f.away) out.push("away");
  if (f.hour < EVENING && !f.today) out.push("today");
  return out;
}
