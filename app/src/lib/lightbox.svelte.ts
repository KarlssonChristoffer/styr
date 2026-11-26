// A picture opened big: over the whole window, as Claude Code does, rather than in a panel beside
// the thread. One <Lightbox /> in App draws whichever is open; anything that shows a picture opens
// it here by its thread and its id in that thread's log.

class Pictures {
  open = $state<{ thread: string; id: string } | null>(null);
}

export const pictures = new Pictures();

/** Opens one of a thread's pictures big; ← → step through the rest of that thread's. */
export function openPicture(thread: string, id: string) {
  pictures.open = { thread, id };
}
