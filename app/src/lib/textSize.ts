// Text size steps (Settings > Appearance), shared by the app and the build's text-size step in
// vite.config.ts, so both agree on how much each step grows.

export type TextSize = "default" | "larger" | "large" | "largest";

/** The size small text grows to at each step (--text-min). */
export const TEXT_MIN: Record<TextSize, number> = { default: 0, larger: 15, large: 16.5, largest: 18 };

/** How much body text (13.5px) grows at a step; boxes that hold text grow by as much (--text-grow). */
export const textGrow = (size: TextSize) => Math.max(13.5, (TEXT_MIN[size] ?? 0) - 0.75) / 13.5;
