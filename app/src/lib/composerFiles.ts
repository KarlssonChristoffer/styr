// The Composer's attachment helpers: reading files, telling a big paste from text, and putting
// an attached text back into the box. Pure, so the message box and its chip row share them.
import type { Attachment } from "./bindings/Attachment";
import { fromFile } from "./attachments";

/** A paste of 32 KB or 300 lines or more (a log, a stack trace) goes along as a text file. */
export const BIG_PASTE = 32 * 1024;
export const BIG_PASTE_LINES = 300;
/** How much of a text file the look at an attachment shows. */
export const PEEK_CHARS = 60_000;

export const isBigPaste = (pasted: string) => new Blob([pasted]).size >= BIG_PASTE || pasted.split("\n").length >= BIG_PASTE_LINES;

export const peekText = (t: string) => (t.length > PEEK_CHARS ? t.slice(0, PEEK_CHARS) + `\n… ${(t.length - PEEK_CHARS).toLocaleString()} more characters` : t);

/** Reads each file into an attachment and hands it to `add`; a file that fails goes to `onerror`. */
export async function readFiles(files: Iterable<File>, takesImages: boolean, add: (a: Attachment) => void, onerror?: (e: unknown) => void) {
  for (const file of files) {
    try {
      const a = await fromFile(file);
      if (a.mime.startsWith("image/") && !takesImages) throw new Error("This engine does not take images");
      add(a);
    } catch (e) {
      onerror?.(e);
    }
  }
}

/** `put` in place of the selection `from`..`to`, on lines of its own so it does not run into what is written. */
export function putInText(text: string, from: number, to: number, insert: string) {
  const before = text.slice(0, from);
  const after = text.slice(to);
  const put = (before && !before.endsWith("\n") ? "\n" : "") + insert + (after && !after.startsWith("\n") ? "\n" : "");
  return { text: before + put + after, caret: before.length + put.length };
}
