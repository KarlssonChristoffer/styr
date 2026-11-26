// Whether the last thing you did was press a key that moves focus (Tab, the arrows) or use the
// mouse. The focus frame (`app.css`) shows only after the first: WebView2 also shows it after a
// click once any key has been pressed, which left a frame around a button or row you had just
// clicked. Shortcuts such as Ctrl N do not count as keyboard use here.

const MOVES = new Set(["Tab", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End"]);

export function installInputMode() {
  const root = document.documentElement;
  const set = (v: "keys" | "mouse") => {
    if (root.dataset.input !== v) root.dataset.input = v;
  };
  addEventListener("keydown", (e) => MOVES.has(e.key) && !e.ctrlKey && !e.metaKey && !e.altKey && set("keys"), true);
  addEventListener("pointerdown", () => set("mouse"), true);
}
