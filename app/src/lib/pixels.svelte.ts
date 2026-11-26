// Whole device pixels for grids of small squares. At 125% zoom (or a 150% Windows screen) one CSS
// pixel is 1.25 device pixels, so a 2px gap and a 14px cell land between pixels and the browser
// rounds each edge its own way: some gaps look wider and darker than their neighbours. Sizes
// that are whole device pixels (`snap`) round the same way everywhere, so the gaps stay even.

const screen = $state({ ratio: window.devicePixelRatio || 1 });
const update = () => (screen.ratio = window.devicePixelRatio || 1);
window.addEventListener("resize", update);
window.addEventListener("hub-zoom", update);

/** The device pixels per CSS pixel now (zoom and the screen's own scale); read it in a $derived to follow zoom. */
export const ratio = () => screen.ratio;

/** `css` pixels as a whole number of device pixels, never fewer than one. */
export const snap = (css: number) => Math.max(1, Math.round(css * screen.ratio)) / screen.ratio;

/** The largest whole-device-pixel size that is at most `css` pixels. */
export const snapDown = (css: number) => Math.max(1, Math.floor(css * screen.ratio + 1e-6)) / screen.ratio;
