// Prompts sent from a box that is not in a thread (Home, a project page), newest last, so Arrow Up
// there brings back what you started threads with. Pictures are left out to keep it small.
import type { Attachment } from "./bindings/Attachment";

/** A message sent before, as Arrow Up brings it back. */
export type Sent = { text: string; attachments: Attachment[] };

const KEY = "hub.sent";
const MAX = 50;

export function recentSent(): Sent[] {
  try {
    const list = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(list) ? list : [];
  } catch {
    return [];
  }
}

export function rememberSent(text: string, attachments: Attachment[]) {
  if (!text.trim()) return;
  const files = attachments.filter((a) => !a.data);
  const list = recentSent().filter((s) => s.text !== text);
  list.push({ text, attachments: files });
  try {
    localStorage.setItem(KEY, JSON.stringify(list.slice(-MAX)));
  } catch {
    // Too big to keep; Arrow Up just has less to go on.
  }
}
