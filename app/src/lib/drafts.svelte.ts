// What you started writing in a thread and did not send, kept per thread so it is there when you
// come back and the activity list can show a pen on the row. Files attached and not sent stay
// with the thread too. Files dropped on a row wait here until that thread's composer opens and
// takes them.
import type { Attachment } from "./bindings/Attachment";

const KEY = "hub.drafts";
const FILES_KEY = "hub.draftFiles";

function load(): Record<string, string> {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "{}");
  } catch {
    return {};
  }
}

export const drafts = $state<Record<string, string>>(load());

function loadFiles(): Record<string, Attachment[]> {
  try {
    return JSON.parse(localStorage.getItem(FILES_KEY) ?? "{}");
  } catch {
    return {};
  }
}

/** Files attached in a thread's box and not sent yet. */
const draftFiles: Record<string, Attachment[]> = loadFiles();

/** Files dropped on a thread's row, for its composer to attach. */
export const dropped = $state<Record<string, Attachment[]>>({});

let timer: ReturnType<typeof setTimeout> | undefined;

export function setDraft(thread: string, text: string) {
  if (text.trim()) drafts[thread] = text;
  else if (thread in drafts) delete drafts[thread];
  else return;
  // Typing writes often; storing once it pauses is enough.
  clearTimeout(timer);
  timer = setTimeout(() => {
    try {
      localStorage.setItem(KEY, JSON.stringify(drafts));
    } catch {
      // The draft still holds until the window closes.
    }
  }, 400);
}

/** Takes the files dropped on a thread's row, once. */
export function takeDropped(thread: string): Attachment[] {
  const files = dropped[thread] ?? [];
  delete dropped[thread];
  return files;
}

export function filesOf(thread: string): Attachment[] {
  return draftFiles[thread] ?? [];
}

export function setDraftFiles(thread: string, files: Attachment[]) {
  if (files.length) draftFiles[thread] = files;
  else if (thread in draftFiles) delete draftFiles[thread];
  else return;
  try {
    localStorage.setItem(FILES_KEY, JSON.stringify(draftFiles));
  } catch {
    // Pictures may not fit; text files are kept, and the rest until the window closes.
    try {
      const light = Object.fromEntries(Object.entries(draftFiles).map(([k, v]) => [k, v.filter((a) => !a.data)]));
      localStorage.setItem(FILES_KEY, JSON.stringify(light));
    } catch {
      // Still there until the window closes.
    }
  }
}
