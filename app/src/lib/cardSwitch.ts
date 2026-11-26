// How the limits card in the name menu changes from one subscription to the next. Morph and
// Ticker change the card in place (bars slide, numbers count, words fade or roll); Slide, Soft
// fade and Deck keep a copy of the old content on top while the new one comes in below it.
import { quiet } from "./quiet";

export type SwitchStyle = "morph" | "slide" | "fade" | "ticker" | "deck";

/** What sits behind the limits card: a quiet backdrop in each company's colours, or nothing. */
export type CardTheme = "glow" | "material" | "edge" | "off";

export const CARD_THEMES: [CardTheme, string][] = [
  ["glow", "Glow"],
  ["material", "Material"],
  ["edge", "Edge"],
  ["off", "Off"],
];

/** The engines that have a backdrop of their own; the rest keep the plain card. */
export const THEMED_ENGINES = ["claude", "codex"];

/** What colours the model picker in the composer: the company's colour follows the provider tab
 *  you are on. Edge is a lit top edge and a faint wash, Glow the same drifting light as the limits
 *  card, Quiet only the tab, the tick and the hover. */
export type PickerTheme = "edge" | "glow" | "quiet" | "off";

export const PICKER_THEMES: [PickerTheme, string][] = [
  ["edge", "Edge"],
  ["glow", "Glow"],
  ["quiet", "Quiet"],
  ["off", "Off"],
];

/** Each themed engine's two colours as `r, g, b`, the same ones the limits card uses. */
export const BRAND_RGB: Record<string, { a: string; b: string }> = {
  claude: { a: "217, 119, 87", b: "204, 160, 112" },
  codex: { a: "110, 140, 255", b: "150, 120, 255" },
};

/** How long a switch takes, also used by the bars' and numbers' own transitions. */
export const SWITCH_MS = 260;
const ease = "cubic-bezier(.2,.7,.2,1)";

export function still() {
  return document.documentElement.classList.contains("calm") || matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** Runs `apply`, which puts the next subscription in the card, and animates the change. */
export async function switchCard(card: HTMLElement, style: SwitchStyle, step: number, apply: () => Promise<void>) {
  if (still()) return apply();
  const from = card.offsetHeight;
  let ghost: HTMLElement | null = null;
  if (style === "slide" || style === "fade" || style === "deck") {
    ghost = document.createElement("div");
    ghost.append(...[...card.children].map((c) => c.cloneNode(true)));
    ghost.inert = true;
    ghost.setAttribute("aria-hidden", "true");
    Object.assign(ghost.style, { position: "absolute", left: "0", top: "0", width: "100%", pointerEvents: "none" });
  }
  await apply();
  const fresh = [...card.children] as HTMLElement[];
  const runs: Promise<unknown>[] = [];
  const to = card.offsetHeight;
  if (from !== to) runs.push(card.animate([{ height: `${from}px` }, { height: `${to}px` }], { duration: SWITCH_MS, easing: ease }).finished);

  const both = (out: Keyframe[], inn: Keyframe[], delay = SWITCH_MS * 0.15) => {
    card.append(ghost!);
    runs.push(ghost!.animate(out, { duration: SWITCH_MS * 0.8, easing: ease, fill: "forwards" }).finished);
    for (const el of fresh) runs.push(el.animate(inn, { duration: SWITCH_MS, easing: ease, delay, fill: "backwards" }).finished);
  };
  const words = () => [...card.querySelectorAll<HTMLElement>("[data-word]")];

  if (style === "morph") {
    for (const el of words()) runs.push(el.animate([{ opacity: 0 }, { opacity: 1 }], { duration: SWITCH_MS * 0.8, easing: "ease-out" }).finished);
  } else if (style === "ticker") {
    words().forEach((el, i) =>
      runs.push(el.animate([{ transform: "translateY(8px)", opacity: 0 }, { transform: "none", opacity: 1 }], { duration: SWITCH_MS * 0.7, easing: ease, delay: i * 30, fill: "backwards" }).finished),
    );
  } else if (style === "slide") {
    const d = 28 * Math.sign(step || 1);
    both([{ transform: "none", opacity: 1 }, { transform: `translateX(${-d}px)`, opacity: 0 }], [{ transform: `translateX(${d}px)`, opacity: 0 }, { transform: "none", opacity: 1 }]);
  } else if (style === "fade") {
    both([{ filter: "blur(0)", opacity: 1 }, { filter: "blur(5px)", opacity: 0 }], [{ filter: "blur(5px)", opacity: 0 }, { filter: "blur(0)", opacity: 1 }]);
  } else if (style === "deck") {
    both([{ transform: "none", opacity: 1 }, { transform: "translateY(10px) scale(.94)", opacity: 0 }], [{ transform: "translateY(12px) scale(.94)", opacity: 0.2 }, { transform: "none", opacity: 1 }], 0);
  }
  await Promise.all(runs.map((r) => quiet(r, "run")));
  ghost?.remove();
}
