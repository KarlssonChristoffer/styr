// The one place the app calls hubd: the merged api object, page URLs and the event stream connection.
import { BASE, streamUrl } from "./home";

import type { StreamMsg } from "./bindings/StreamMsg";
import { threadApi } from "./apiThreads";
import { projectApi } from "./apiProjects";
import { customsApi } from "./apiGithub";

export { BASE };
/** Where the core serves a live page a thread's agent showed, sandboxed (`pages.rs`). */
export const pageUrl = (thread: string, id: string) => `${BASE}/api/threads/${encodeURIComponent(thread)}/pages/${encodeURIComponent(id)}`;

export type { PhoneNotify } from "./apiThreads";
export type { NewProjectMore } from "./apiProjects";

/** Every call the app makes to a hubd, by name. Grouped in `apiThreads.ts`, `apiProjects.ts` and `apiGithub.ts`. */
export const api = { ...threadApi, ...projectApi, ...customsApi };

export type StreamItem = StreamMsg;

/** Keeps a WebSocket to the core open, reconnecting with backoff. */
export function connectStream(handlers: {
  message: (msg: StreamItem) => void;
  open: () => void;
  close: () => void;
}): () => void {
  let ws: WebSocket | null = null;
  let delay = 250;
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const open = () => {
    ws = new WebSocket(streamUrl());
    ws.onopen = () => {
      delay = 250;
      handlers.open();
    };
    ws.onmessage = (e) => {
      let item: StreamItem;
      try {
        item = JSON.parse(e.data);
      } catch (err) {
        // One bad frame must not stop the ones after it.
        console.debug("stream: unreadable message", err);
        return;
      }
      handlers.message(item);
    };
    ws.onclose = () => {
      handlers.close();
      if (!stopped) timer = setTimeout(open, (delay = Math.min(delay * 2, 5000)));
    };
  };
  open();
  return () => {
    stopped = true;
    clearTimeout(timer);
    ws?.close();
  };
}
