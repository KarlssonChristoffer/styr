/** The themes. `dark` is the default (no attribute on <html>); every other id is a
 *  `:root[data-theme="<id>"]` block in app.css. The swatch colours are only for the picker in
 *  Settings > Appearance; the real tokens are in app.css. A client that meets an id it does not
 *  know falls back to the default tokens, so adding a theme never breaks an older Hub. */
export type Theme = "dark" | "cappuccino" | "coal" | "graphite" | "ink" | "forest" | "aubergine" | "smoke" | "neutral" | "paper" | "sage";

export interface ThemeInfo { id: Theme; label: string; light: boolean; bg: string; surface: string; accent: string; text: string }

export const THEMES: ThemeInfo[] = [
  { id: "dark", label: "Dark", light: false, bg: "#1a1918", surface: "#211f1e", accent: "#a5abee", text: "#ece8e2" },
  { id: "coal", label: "Coal", light: false, bg: "#15110c", surface: "#1b1611", accent: "#a2a8ff", text: "#eee6e0" },
  { id: "graphite", label: "Graphite", light: false, bg: "#0f1216", surface: "#15171c", accent: "#8baffe", text: "#e4e8ef" },
  { id: "ink", label: "Ink", light: false, bg: "#0d1218", surface: "#13181e", accent: "#c0a0e7", text: "#e2e9f1" },
  { id: "forest", label: "Forest", light: false, bg: "#0c1410", surface: "#111916", accent: "#bfb17e", text: "#e0ebe5" },
  { id: "aubergine", label: "Aubergine", light: false, bg: "#160f14", surface: "#1c141a", accent: "#dd96cd", text: "#efe4ec" },
  { id: "smoke", label: "Smoke", light: false, bg: "#121111", surface: "#181716", accent: "#b5b0ac", text: "#e9e7e6" },
  { id: "neutral", label: "Neutral", light: false, bg: "#141415", surface: "#19191b", accent: "#7fb2f0", text: "#ededed" },
  { id: "cappuccino", label: "Cappuccino", light: true, bg: "#f1e9df", surface: "#e5dace", accent: "#484c9a", text: "#2a211b" },
  { id: "paper", label: "Paper", light: true, bg: "#f3f7fc", surface: "#ecf1f5", accent: "#3658ac", text: "#1f2225" },
  { id: "sage", label: "Sage", light: true, bg: "#f1faf2", surface: "#eaf3eb", accent: "#1d6c5b", text: "#1d241e" },
];

/** Puts the theme on <html> so app.css swaps the tokens. */
export function applyTheme(id: string) {
  const root = document.documentElement;
  if (id === "dark") root.removeAttribute("data-theme");
  else root.dataset.theme = id;
}
