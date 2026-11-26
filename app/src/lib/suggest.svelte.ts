// The composer's suggestion popup (file matches, slash commands): state and keyboard handling.
import type { FileMatch } from "./bindings/FileMatch";
import { hub } from "./state.svelte";
import { quiet } from "./quiet";

/** `remove` lets the Delete key take the row away, as with a put-away prompt. */
export type Suggestion = { label: string; detail: string; hub?: boolean; thread?: boolean; apply: () => void; remove?: () => void };

/** What the suggestions need from the message box that owns them. Every member reads the box as it is now. */
export type SuggestHost = {
  text: () => string;
  setText: (text: string) => void;
  /** The caret's place in the text, or null while the box is not there. */
  caret: () => number | null;
  /** Puts the caret at a place and the focus in the box. */
  focusAt: (at: number) => void;
  threadId: () => string | undefined;
  inThread: () => boolean;
  /** Hub's own commands; the ones that cannot run here are left out already. */
  hubCommands: () => { name: string; description: string; run: () => void }[];
  /** The engine's commands, which only work at the start of a message. */
  engineCommands: () => { name: string; description: string; hint?: string | null }[];
  findFiles: () => ((query: string) => Promise<FileMatch[]>) | undefined;
  mention: (m: FileMatch) => void;
  refer: (t: { id: string; title: string }) => void;
};

/** The list above the box for `/` commands and `@` files and threads. */
export class Suggest {
  list = $state<Suggestion[]>([]);
  active = $state(0);
  private fileQuery = 0;

  constructor(private host: SuggestHost) {}

  private caretContext() {
    const text = this.host.text();
    const before = text.slice(0, this.host.caret() ?? text.length);
    const command = /(^|\s)\/(\S*)$/.exec(before);
    const file = /(^|\s)@([^\s@]*)$/.exec(before);
    return { before, command, file };
  }

  clear() {
    this.list = [];
  }

  refresh() {
    const h = this.host;
    const { before, command, file } = this.caretContext();
    if (command) {
      const q = command[2].toLowerCase();
      const first = (a: { name: string }, b: { name: string }) =>
        Number(!a.name.toLowerCase().startsWith(q)) - Number(!b.name.toLowerCase().startsWith(q));
      const typed = before.length - q.length - 1;
      const hubCommands = h.hubCommands();
      const ours: Suggestion[] = hubCommands
        .filter((c) => c.name.includes(q))
        .map((c) => ({
          label: "/" + c.name,
          detail: c.description,
          hub: true,
          apply: () => {
            // The command leaves the text; what was written around it stays.
            this.replaceBefore(before, before.slice(0, typed).replace(/\s$/, ""));
            queueMicrotask(() => c.run());
          },
        }));
      // The engine's own commands only work at the start of a message.
      const theirs: Suggestion[] = h.inThread() && typed === 0
        ? h.engineCommands()
            .filter((c) => c.name.toLowerCase().includes(q) && !hubCommands.some((x) => x.name === c.name))
            .sort(first)
            .map((c) => ({
              label: "/" + c.name + (c.hint ? ` ${c.hint}` : ""),
              detail: c.description,
              apply: () => this.replaceBefore(before, "/" + c.name + " "),
            }))
        : [];
      this.list = [...ours, ...theirs].slice(0, 12);
      this.active = 0;
    } else if (file) {
      const q = file[2];
      const ticket = ++this.fileQuery;
      // Other threads by title, from two letters on; the files follow.
      const threads: Suggestion[] = q.length >= 2
        ? hub.threads
            .filter((t) => hub.inWorld(t) && t.id !== h.threadId() && !t.archived && t.title.toLowerCase().includes(q.toLowerCase()))
            .slice(0, 4)
            .map((t) => ({
              label: t.title,
              detail: "thread",
              thread: true,
              apply: () => {
                const start = before.length - q.length - 1;
                this.replaceBefore(before, before.slice(0, start) + "@" + t.title + " ");
                h.refer({ id: t.id, title: t.title });
              },
            }))
        : [];
      const findFiles = h.findFiles();
      if (!findFiles) {
        this.list = threads;
        this.active = 0;
        return;
      }
      setTimeout(async () => {
        if (ticket !== this.fileQuery) return;
        const found = await quiet(findFiles(q), "find files", []);
        if (ticket !== this.fileQuery) return;
        this.list = [...threads, ...found.slice(0, 12 - threads.length).map((m) => ({
          label: m.rel + (m.dir ? "/" : ""),
          detail: m.dir ? "folder" : "",
          apply: () => {
            const start = before.length - q.length - 1;
            this.replaceBefore(before, before.slice(0, start) + "@" + m.rel + " ");
            h.mention(m);
          },
        }))];
        this.active = 0;
      }, 90);
    } else {
      this.fileQuery++;
      this.list = [];
    }
  }

  /** Replaces the text before the caret, keeping what follows it. */
  private replaceBefore(before: string, replacement: string) {
    const after = this.host.text().slice(before.length);
    this.host.setText(replacement + after);
    this.list = [];
    queueMicrotask(() => this.host.focusAt(replacement.length));
  }

  /** Arrows, Enter, Tab, Esc and Delete while the list is open. True when the key was taken. */
  key(e: KeyboardEvent): boolean {
    if (!this.list.length) return false;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = this.list.length;
      this.active = (this.active + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
      return true;
    }
    if ((e.key === "Enter" || e.key === "Tab") && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      this.list[this.active]?.apply();
      return true;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      this.list = [];
      return true;
    }
    if (e.key === "Delete" && this.list[this.active]?.remove) {
      e.preventDefault();
      this.list[this.active].remove!();
      return true;
    }
    return false;
  }
}
