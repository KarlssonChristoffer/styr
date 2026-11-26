// Threads that run on Anthropic's or OpenAI's servers instead of a machine of yours (a Claude
// Code cloud session, or a Codex cloud task), always on the subscription's own login. The core
// starts them with the CLI and keeps their status (`cloud` on the thread); this file is what the
// window knows about them. Nothing here is needed to read a thread: a core without cloud threads
// never sets `cloud`, and the window then shows nothing of it.

import type { Thread } from "./bindings/Thread";
import { hubdFeatures } from "./compat";
import { engineProvider } from "./providers.svelte";

export type CloudProvider = "claude" | "codex";

/** Where a cloud thread stands, as the core last read it. An unknown state reads as working. */
export type CloudState = "starting" | "working" | "waiting" | "done" | "failed" | "stopped";

export interface CloudRun {
  provider: CloudProvider;
  session_id: string;
  /** The session's page on claude.ai or chatgpt.com. */
  url?: string | null;
  /** `owner/name` on GitHub. */
  repo?: string | null;
  branch?: string | null;
  state: CloudState | (string & {});
  /** The last thing the session said it was doing, when the service lets Hub read it. */
  last?: string | null;
  pr?: { number: number; title?: string | null; url: string; additions?: number | null; deletions?: number | null } | null;
  /** Why it could not start or be read, in words for the thread. */
  note?: string | null;
}

/** The cloud run of a thread, or null for a thread on a machine. */
export function cloudOf(t: Pick<Thread, "id"> | null | undefined): CloudRun | null {
  return ((t as { cloud?: CloudRun | null } | null | undefined)?.cloud ?? null) as CloudRun | null;
}

/** Which cloud an engine would start a thread in: the provider of the model it runs. The Claude
 *  and Codex harnesses on their own subscription only; Hermes or Claude Code on DeepSeek have none. */
export function cloudProviderFor(engine: string): CloudProvider | null {
  const id = engineProvider(engine).id;
  return id === "anthropic" ? "claude" : id === "openai" ? "codex" : null;
}

export const cloudLabel = (p: CloudProvider) => (p === "claude" ? "Claude cloud" : "Codex cloud");
export const cloudHost = (p: CloudProvider) => (p === "claude" ? "Anthropic's servers" : "OpenAI's servers");

/** Whether the hubd this window uses can start cloud threads (`threads.cloud` in its features). A
 *  hubd that does not say, or cannot be asked, counts as not: a thread must never start on this
 *  machine when you asked for the cloud. */
export const cloudSupport = $state({ asked: false, has: false });

export function askCloudSupport() {
  if (cloudSupport.asked) return;
  cloudSupport.asked = true;
  void hubdFeatures().then((f) => {
    cloudSupport.has = !!f?.has("threads.cloud");
    if (f === null) cloudSupport.asked = false;
  });
}

/** The state in words, for rows and the card. */
export function cloudStateLabel(c: CloudRun): string {
  switch (c.state) {
    case "starting": return "Starting";
    case "waiting": return "Your turn";
    case "done": return "Done";
    case "failed": return "Failed";
    case "stopped": return "Stopped";
    default: return "Working";
  }
}

/** How far a cloud thread has come, 1 to 4: started, working, branch pushed, pull request open. */
export function cloudStep(c: CloudRun): number {
  if (c.pr) return 4;
  if (c.branch && (c.state === "done" || c.state === "waiting")) return 3;
  return c.state === "starting" ? 1 : 2;
}
