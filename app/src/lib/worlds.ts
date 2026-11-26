// The two worlds (Dev and Chat) and the two rooms (Work and Control): which one is on screen, and moving between them. The calls live on `hub`; these hold what they do.
import { prefs, setPref } from "./prefs.svelte";
import { byProject, folderName, isChatThread } from "./format";
import { needsYou } from "./threadFlags";
import type { Room, World } from "./viewTypes";
import type { HubState } from "./state.svelte";

/** The wall between the worlds. A world changes only when you ask (the rail's button, Alt 3) or
 *  when you open a thread by itself from somewhere outside both lists (a notification, a link):
 *  then the thread's own world comes with it. No other page changes it. Chat has two places, a
 *  chat and New chat; any page of Dev's that ends up on screen in Chat (back and forward, a
 *  stale key) is sent to New chat instead of taking the window into Dev. Called whenever the
 *  page changes. */
export function syncWorld(hub: HubState) {
  if (hub.view === "thread") {
    const want: World | null = hub.current ? (isChatThread(hub.current) ? "chat" : "dev") : null;
    if (want && want !== hub.world) hub.setWorld(want);
    return;
  }
  if (hub.world === "chat" && hub.view !== "new") hub.goNew();
}

export function setWorld(hub: HubState, to: World) {
  hub.world = to;
  setPref("world", to);
}

/** Goes to the other world (or `to`), on the page it had open: its last thread, else New chat in
 *  Chat and Home in Dev. */
export function switchWorld(hub: HubState, to: World = hub.world === "dev" ? "chat" : "dev") {
  if (to === hub.world) return;
  hub.worldRoute[hub.world] = hub.route;
  let next = hub.worldRoute[to];
  if (next?.view === "thread" && !hub.threads.some((t) => t.id === (next as { thread: string }).thread)) next = null;
  hub.world = to;
  setPref("world", to);
  if (next) return hub.show(next);
  if (to === "chat") hub.goNew();
  else hub.goHome();
}

/** A thread that exists and belongs to the world on screen. A room remembers the thread you had
 *  open in it, and a chat opened from Chat is remembered by Inbox too; going into Inbox from Dev
 *  must not take you to that chat, and so into Chat. */
export function threadHere(hub: HubState, id: string): boolean {
  const t = hub.threads.find((x) => x.id === id);
  return !!t && isChatThread(t) === (hub.world === "chat");
}

/** Goes to the other room (or `to`), on the page it had open. Leaving a project page for Work
 *  narrows Work to that project's threads. */
export function switchRoom(hub: HubState, to: Room = hub.room === "work" ? "control" : "work") {
  if (hub.world === "chat" || to === hub.room) return;
  if (to === "work" && hub.view === "project" && hub.project && !prefs.focusOn) {
    const name = byProject([], hub.projects).find((p) => p.cwd === hub.project)?.name ?? folderName(hub.project);
    hub.workScope = { cwd: hub.project, host: hub.projectHost, name };
  }
  let next = hub.rooms[to];
  if (next.view === "thread" && !hub.threadHere((next as { thread: string }).thread)) next = { view: "home" };
  hub.show(next);
}

/** Goes into a room from the rail. Into the room you are in from Home, it opens the thread you
 *  had open there, else the first that needs you. */
export function enterRoom(hub: HubState, to: Room) {
  if (hub.world === "chat") return;
  if (to !== hub.room) return hub.switchRoom(to);
  if (to !== "work" || (hub.view !== "home" && hub.view !== "new")) return;
  const last = hub.rooms.work;
  if (last.view === "thread" && hub.threadHere((last as { thread: string }).thread)) return hub.show(last);
  const first = hub.inbox.find(needsYou) ?? hub.inbox[0] ?? hub.active[0];
  if (first) hub.open(first.id);
}
