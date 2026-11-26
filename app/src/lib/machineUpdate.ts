// How a machine's update looks on screen: the step, dots, tone and tip for each stage.
import { shortVersion } from "./format";
// The update control on a machine's card (MachineCard.svelte): what the other machine's updater
// shows, as one step out of five (sent, received, building, ready, restarting), the way Settings >
// Updates shows a sent version. The steps only move forward within one update.
import type { UpdateStatus } from "./bindings/UpdateStatus";

/** What the card draws at its top right. */
export type UpdateControl = {
  /** The step the update is at, as a word; none when there is nothing running. */
  step: string | null;
  /** How many of the five dots are lit. */
  dots: number;
  tone: "go" | "work" | "ok" | "bad";
  /** The button's words, or null when there is no button. */
  button: string | null;
  busy: boolean;
  tip: string;
  run: () => void;
};

export type Stage = { step: string; dots: number; tone: UpdateControl["tone"]; tip: string };


/** The step from what the machine's updater says, or null when it says nothing about an update.
 * `sending`: this computer is building or sending it; `sent`: it was sent, and the machine has not
 * shown it yet; `updated`: the machine now runs it (`to`). */
export function stageOf(o: { name: string; upd: UpdateStatus | null; sending: string | null | undefined; sent: boolean; updatedTo: string | null }): Stage | null {
  if (o.sending) return { step: "sending", dots: 0, tone: "go", tip: `Updating: ${o.sending}…` };
  const u = o.upd;
  const target = u?.target ? ` ${shortVersion(u.target)}` : "";
  switch (u?.state) {
    case "checking":
      return { step: "received", dots: 2, tone: "work", tip: `${o.name} has it and is fetching the new version` };
    case "building":
      return { step: "building", dots: 3, tone: "work", tip: `${o.name} is building hubd${target}${u.step ? `: ${u.step}` : ""}. It switches by itself when done.` };
    case "ready":
      return { step: "ready", dots: 4, tone: "go", tip: `hubd${target} is built on ${o.name}. It switches once no thread there is working.` };
    case "restarting":
      return { step: "restarting", dots: 5, tone: "go", tip: `${o.name} is restarting into hubd${target}` };
    case "failed":
      return { step: "failed", dots: 0, tone: "bad", tip: `The update on ${o.name} failed${u.message ? `: ${u.message.split("\n")[0]}` : "."}` };
  }
  if (o.updatedTo) return { step: "updated", dots: 5, tone: "ok", tip: `Updated to hubd ${shortVersion(o.updatedTo)}` };
  if (o.sent) {
    if (u?.state === "disabled") return { step: "failed", dots: 1, tone: "bad", tip: `${o.name} has updates switched off, so it does not take the new version. Install hubd again from Settings > Machines.` };
    return { step: "sent", dots: 1, tone: "go", tip: `Sent. Waiting for ${o.name} to start building.` };
  }
  return null;
}
