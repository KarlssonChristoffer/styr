// A page opened big, over the window: one <Big /> in App draws whichever is open. Anything that
// shows a page opens it here by its thread and its id in that thread's log, as pictures do with
// `lightbox.svelte.ts`. (Named apart from `Big.svelte`: a file system that ignores case, as on Windows, would otherwise read `big.svelte` as the component.)

class BigView {
  /** A page by its id, or a file by its path. */
  open = $state<{ thread: string; id?: string; file?: string; text?: string | null } | null>(null);
}

export const bigView = new BigView();

/** Opens one of a thread's pages big; ← → step through its other versions. */
export function openBig(thread: string, id: string) {
  bigView.open = { thread, id };
}

/** Opens a file a thread's agent made big, as it reads; `text` is what the reply wrote when it is known. */
export function openBigFile(thread: string, file: string, text: string | null = null) {
  bigView.open = { thread, file, text };
}
