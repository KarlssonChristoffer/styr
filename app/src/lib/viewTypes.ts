// The view, room, world and tab names the router and the state share.
import type { ShownItem } from "./transcript";

export type View = "home" | "new" | "thread" | "services" | "workers" | "machines" | "machine" | "project" | "labs" | "welcome" | "overview" | "problems" | "routines" | "recipes" | "library" | "plugins";
export type SettingsSection =
  | "account" | "spend" | "appearance" | "notifications" | "dictation"
  | "chat" | "engines" | "handoffs" | "personality" | "safety" | "customs" | "projects"
  | "extensions" | "apis" | "vault"
  | "machines" | "access"
  | "updates" | "whatsnew" | "privacy" | "about";
export type ProjectTab = "overview" | "library" | "github" | "docs" | "files" | "services" | "tests" | "rules";
/** Hub has two rooms. Work is where threads get done; Control is where you organise projects
 *  and keep an eye on the system. Each view belongs to one, and the sidebar shows that room. */
export type Room = "work" | "control";
/** Hub has two worlds. Dev is the work: threads, projects, machines. Chat is the iPhone's everyday
 *  chats, in warm colours and with nothing of the code side. A world follows the page you are
 *  on (`syncWorld`); the world button on the rail changes it (`switchWorld`). */
export type World = "dev" | "chat";
export const roomOf = (view: View): Room => (view === "thread" || view === "home" || view === "new" ? "work" : "control");
/** Where you are, as back and forward remember it. Only what tells two places apart. */
export type Route =
  | { view: "thread"; thread: string }
  | { view: "project"; project: string; host: string | null; tab: ProjectTab; doc?: string | null; item?: string | null }
  | { view: "library"; doc: string | null }
  | { view: "plugins"; repo: string | null }
  | { view: "machine"; machine: string }
  | { view: Exclude<View, "thread" | "project" | "library" | "machine" | "plugins"> };

/** What the viewer shows: a kind of thing an agent showed, and its id among the thread's items;
 *  or, as "browser", a dev server beside the thread, its id the URL; or, as "bag", an item from
 *  the context bag, by its id. */
/** What the viewer shows. A "file" is a path, absolute or in the thread's folder, read by FilePreview. */
export type Viewing = { thread: string; kind: ShownItem["kind"] | "browser" | "bag" | "file" | "sim" | "tests"; id: string };

/** Somewhere in the main window to go: a Settings section or a page. */
export type GoTarget = { settings: SettingsSection } | { view: View } | { project: string; host: string | null; tab: ProjectTab };
