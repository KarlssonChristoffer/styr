// Command output as a terminal would show it: colours and bold from ANSI escapes, a carriage
// return rewriting its line (progress bars), every other escape dropped. Colours map to the
// theme's tokens (`.c-*` in TerminalOutput.svelte), so output stays calm in both themes.

export type Span = { text: string; cls: string };
export type Line = Span[];

/** The last `max` lines of `text`, and how many came before them. */
export function screen(text: string, max: number): { lines: Line[]; earlier: number } {
  // Only the tail is parsed: a long build's output is megabytes and only its end is shown.
  let start = text.length;
  let found = 0;
  const end = text.endsWith("\n") ? text.length - 1 : text.length;
  start = end;
  while (found < max) {
    const nl = text.lastIndexOf("\n", start - 1);
    if (nl < 0) {
      start = 0;
      break;
    }
    start = nl;
    found++;
  }
  const lines = parse(text.slice(start === 0 ? 0 : start + 1, end));
  const earlier = start === 0 ? 0 : count(text, start);
  return { lines: lines.slice(-max), earlier };
}

function count(text: string, before: number): number {
  let n = 0;
  for (let i = text.indexOf("\n"); i >= 0 && i <= before; i = text.indexOf("\n", i + 1)) n++;
  return n;
}

const COLOURS: Record<number, string> = {
  30: "dim", 31: "red", 32: "green", 33: "amber", 34: "blue", 35: "magenta", 36: "cyan", 37: "",
  90: "dim", 91: "red", 92: "green", 93: "amber", 94: "blue", 95: "magenta", 96: "cyan", 97: "",
};
/** The 16 basic colours of a 256-colour code, as the codes above. */
const BASIC = [30, 31, 32, 33, 34, 35, 36, 37, 90, 91, 92, 93, 94, 95, 96, 97];

export function parse(text: string): Line[] {
  const lines: Line[] = [];
  let line: Line = [];
  let buf = "";
  let fg = "";
  let bold = false;
  let faint = false;
  const cls = () => [fg && `c-${fg}`, bold && "b", faint && "d"].filter(Boolean).join(" ");
  let style = cls();
  const flush = () => {
    if (!buf) return;
    const last = line.at(-1);
    if (last && last.cls === style) last.text += buf;
    else line.push({ text: buf, cls: style });
    buf = "";
  };
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (ch === "\n") {
      flush();
      lines.push(line);
      line = [];
    } else if (ch === "\r") {
      // "\r\n" ends the line; a lone "\r" starts it again, as a progress bar redraws itself.
      if (text[i + 1] === "\n") continue;
      flush();
      if (i + 1 < text.length) line = [];
    } else if (ch === "\b") {
      if (buf) buf = buf.slice(0, -1);
    } else if (ch === "\x1b") {
      const next = text[i + 1];
      if (next === "[") {
        let j = i + 2;
        while (j < text.length && !/[@-~]/.test(text[j])) j++;
        if (text[j] === "m") {
          flush();
          const codes = text.slice(i + 2, j).split(/[;:]/).map((c) => (c === "" ? 0 : Number(c)));
          for (let k = 0; k < codes.length; k++) {
            const c = codes[k];
            if (c === 0) (fg = ""), (bold = false), (faint = false);
            else if (c === 1) bold = true;
            else if (c === 2) faint = true;
            else if (c === 22) (bold = false), (faint = false);
            else if (c === 39) fg = "";
            else if (c in COLOURS) fg = COLOURS[c];
            else if (c === 38 && codes[k + 1] === 5) {
              const n = codes[k + 2];
              fg = n != null && n < 16 ? COLOURS[BASIC[n]] : fg;
              k += 2;
            } else if (c === 38 && codes[k + 1] === 2) k += 4;
            else if (c === 48 && codes[k + 1] === 5) k += 2;
            else if (c === 48 && codes[k + 1] === 2) k += 4;
          }
          style = cls();
        }
        i = j;
      } else if (next === "]") {
        // An operating system command (a window title, a link), ended by BEL or ESC \.
        let j = i + 2;
        while (j < text.length && text[j] !== "\x07" && !(text[j] === "\x1b" && text[j + 1] === "\\")) j++;
        i = text[j] === "\x1b" ? j + 1 : j;
      } else {
        i++;
      }
    } else if (ch >= " " || ch === "\t") {
      buf += ch;
    }
  }
  flush();
  if (line.length || text.length) lines.push(line);
  return lines;
}

/** The output as plain text, for copying. */
export function plain(text: string): string {
  return parse(text)
    .map((l) => l.map((s) => s.text).join(""))
    .join("\n");
}
