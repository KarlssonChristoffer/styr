// Starting things in the main window, also from a viewer popped out into its own window: a draft, attachments for a thread, a prompt link, a place to go. The calls live on `hub`; these hold what they do.
import { quiet } from "./quiet";
import { focusWindow } from "./native";
import type { Attachment } from "./bindings/Attachment";
import type { PromptLink } from "./bindings/PromptLink";
import type { Thread } from "./bindings/Thread";
import { folderName, isChatThread } from "./format";
import type { GoTarget } from "./viewTypes";
import type { HubState } from "./state.svelte";

/** From viewers popped out into their own windows to the main window (same origin, same app). */
const toMain = new BroadcastChannel("hub-main");

/** Opens New thread with a message ready to send, or one asking about all of Hub. */
export function startDraft(hub: HubState, cwd: string, text: string, host: string | null = null, attachments?: Attachment[], everything?: boolean, from?: string | null) {
  // A chat's folder is never the folder of a dev draft: it would start a chat from Dev's box.
  // The other way round, a dev folder asked for from Chat (a link) takes you into Dev.
  if (isChatThread({ cwd }) && hub.world !== "chat") cwd = "";
  else if (cwd && !isChatThread({ cwd }) && hub.world === "chat") hub.setWorld("dev");
  if (hub.popped) return void toMain.postMessage({ draft: { cwd, text, host, attachments, everything, from } });
  hub.draft = { cwd, text, host, attachments, everything, from };
  hub.goNew();
}

/** Opens a thread with attachments in its message box, to go with what you write next. */
export function stageIn(hub: HubState, id: string, attachments: Attachment[]) {
  if (hub.popped) return void toMain.postMessage({ stage: { id, attachments } });
  hub.staged = { ...hub.staged, [id]: [...(hub.staged[id] ?? []), ...attachments] };
  hub.open(id);
}

/** A prompt link was opened: its text goes in a new message box, in the folder it names or the
 *  project with the repository's name, to read before sending. */
export function fromLink(hub: HubState, link: PromptLink) {
  if (hub.popped) return;
  const name = link.repo?.split("/").filter(Boolean).at(-1)?.replace(/\.git$/, "").toLowerCase();
  const cwd =
    link.folder ??
    (name ? hub.projects.find((p) => folderName(p.path).toLowerCase() === name || p.name.toLowerCase() === name)?.path : undefined) ??
    hub.draftFolder();
  hub.startDraft(cwd, link.prompt, null);
  quiet(focusWindow(), "focus window");
  if (link.stripped) hub.fail(new Error("The link had hidden characters in it, taken out. It may be trying to sneak in instructions: read it before sending."));
  else if (name && !hub.projects.some((p) => p.path === cwd)) hub.tell(`No project for ${link.repo} here; pick the folder`);
}

/** The folder a new draft starts in: the open thread's, or the newest thread's. */
export function draftFolder(hub: HubState): string {
  const work = (t: Thread | null | undefined) => (t && !isChatThread(t) ? t.cwd : undefined);
  return work(hub.current) ?? hub.threads.find((t) => work(t))?.cwd ?? "";
}

/** Takes what a popped-out viewer starts; returns a stop. `focus` brings this window forward. */
export function fromPopped(hub: HubState, focus: () => void) {
  toMain.onmessage = (e: MessageEvent) => {
    const { stage, draft, go } = e.data ?? {};
    if (go) hub.goTo(go as GoTarget);
    else if (stage) hub.stageIn(stage.id, stage.attachments);
    else if (draft) hub.startDraft(draft.cwd, draft.text, draft.host, draft.attachments, draft.everything, draft.from);
    else return;
    focus();
  };
  return () => (toMain.onmessage = null);
}

/** Takes the main window to a place. From a popped-out window it asks the main one to. */
export function goTo(hub: HubState, t: GoTarget) {
  if (hub.popped) return void toMain.postMessage({ go: t });
  if ("settings" in t) return hub.openSettings(t.settings);
  hub.settingsOpen = false;
  if ("view" in t) {
    hub.view = t.view;
    hub.selected = null;
  } else {
    hub.openProject(t.project, t.host);
    hub.projectTab = t.tab;
  }
}
