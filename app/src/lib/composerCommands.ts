// Hub's own slash commands in the composer (/diff, /tidy, /review): typed like the engine's commands,
// but they run here instead of going to the engine. Used by Composer.svelte.
import { api } from "./api";
import type { Attachment } from "./bindings/Attachment";

/** What a command needs from the box it was typed in. */
export type CommandBox = {
  cwd?: string;
  host?: string | null;
  threadId?: string;
  inThread: boolean;
  attach: (a: Attachment) => void;
  text: () => string;
  tidy: () => void;
  onerror?: (e: unknown) => void;
};

/** Attaches the folder's uncommitted changes as a diff. */
export async function attachDiff(box: CommandBox) {
  if (!box.cwd) return;
  try {
    const { text: patch } = await api.diff(box.cwd, box.host);
    if (!patch) throw new Error("No uncommitted changes in this folder");
    box.attach({ name: "uncommitted.diff", mime: "text/x-diff", text: patch });
  } catch (e) {
    box.onerror?.(e);
  }
}

export function hubCommandsFor(box: CommandBox) {
  const { cwd, threadId, onerror } = box;
  async function askReview() {
    if (!threadId) return;
    try {
      await api.review(threadId);
    } catch (e) {
      onerror?.(e);
    }
  }
  return [
    { name: "diff", description: "Attach the folder's uncommitted changes", ok: !!cwd, run: () => attachDiff(box) },
    {
      name: "tidy",
      description: "Rewrite what you wrote as a clear prompt",
      ok: !!cwd,
      run: () => (box.text().trim() ? box.tidy() : onerror?.(new Error("Write or dictate something first, then /tidy"))),
    },
    { name: "review", description: "Get a second opinion on the work so far", ok: box.inThread && !!threadId, run: askReview },
  ].filter((c) => c.ok);
}
