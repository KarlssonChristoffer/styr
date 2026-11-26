// Things only the desktop shell can do, with browser fallbacks where there is one.
import { quiet } from "./quiet";
import { windowLabel } from "./carry";
import type { Viewing } from "./state.svelte";

export const inTauri = "__TAURI_INTERNALS__" in window;

export async function pickFolder(start?: string): Promise<string | null> {
  if (!inTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ directory: true, defaultPath: start || undefined, title: "Folder for the thread" });
  return typeof picked === "string" ? picked : null;
}

/** Moves the app to the version the core just installed. False when there is none. */
export async function relaunch(): Promise<boolean> {
  if (!inTauri) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<boolean>("relaunch");
}

/** Lets the window the core opens next, such as Explorer showing a file, come to the front.
 *  Windows keeps it behind Hub otherwise, since the core is not the app you are using. */
export async function letForeground() {
  if (!inTauri) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await quiet(invoke("let_foreground"), "invoke");
}

/** Pops a viewer out of this window into one of its own (`ViewerWindow.svelte`). */
export async function popOut(viewing: Viewing) {
  if (!inTauri) return void window.open(`${location.pathname}?viewer=${encodeURIComponent(JSON.stringify(viewing))}`, "_blank", "popup,width=1000,height=760");
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("open_viewer_window", { viewing: JSON.stringify(viewing) });
}

/** What a popped-out viewer window shows, or null in the main window. */
export function poppedViewing(): Viewing | null {
  const given = (window as { __HUB_VIEWER__?: Viewing }).__HUB_VIEWER__;
  if (given) return given;
  const query = new URLSearchParams(location.search).get("viewer");
  try {
    return query ? JSON.parse(query) : null;
  } catch {
    return null;
  }
}

/** Brings this window to the front. */
export async function focusWindow() {
  if (!inTauri) return window.focus();
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().setFocus();
}

/** Puts the number of threads waiting on you in the window and taskbar title. */
export async function setTitle(title: string) {
  document.title = title;
  if (!inTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().setTitle(title);
}

/** Flashes Hub in the taskbar until you look at it, when it is not the window in front. */
export async function attention() {
  if (!inTauri || document.hasFocus()) return;
  const { getCurrentWindow, UserAttentionType } = await import("@tauri-apps/api/window");
  await getCurrentWindow().requestUserAttention(UserAttentionType.Informational);
}

let allowed: boolean | null = null;

async function permission(): Promise<boolean> {
  if (allowed !== null) return allowed;
  if (inTauri) {
    const n = await import("@tauri-apps/plugin-notification");
    allowed = (await n.isPermissionGranted()) || (await n.requestPermission()) === "granted";
  } else if ("Notification" in window) {
    allowed = Notification.permission === "granted" || (await Notification.requestPermission()) === "granted";
  } else {
    allowed = false;
  }
  return allowed;
}

/** Shows a system notification, but only when Hub is not the window you are looking at (`force`
 *  shows it anyway, for the test in Settings). Says whether Windows may show one. */
export async function notify(title: string, body: string, force = false): Promise<boolean> {
  if (!force && document.hasFocus()) return false;
  if (!(await permission())) return false;
  if (inTauri) {
    const n = await import("@tauri-apps/plugin-notification");
    n.sendNotification({ title, body });
  } else {
    new Notification(title, { body });
  }
  return true;
}

/** Registers Hub with Windows for notices and says what would keep one from showing; empty when
 *  nothing is known to, and outside the desktop shell or in a shell without the call. */
export async function toastCheck(): Promise<string> {
  if (!inTauri) return "";
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<string>("toast_check");
  } catch {
    return "";
  }
}

/** Opens a web link in the browser. The desktop shell asks the core, which runs on the same machine. */
export async function openUrl(url: string) {
  if (!inTauri) return void window.open(url, "_blank", "noopener");
  const { api } = await import("./api");
  await api.openUrl(url);
}

/** True in the dev window, which runs the dev mode clone's UI with hot reload. The main window
 *  showing the same pages in Live mode is still the main window. */
export const isDevWindow = import.meta.env.DEV && location.port === "1421" && windowLabel() !== "main";

/** Opens dev mode's live UI in a window of its own, or a browser tab outside the desktop shell. */
export async function openDevWindow(url: string) {
  if (!inTauri) return void window.open(url, "_blank", "noopener");
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("open_dev_window", { url });
}

/** Hub draws its own title bar in the desktop shell: the window has no Windows frame. */
export const ownTitleBar = inTauri;

const controls = "button, a, input, textarea, select, [role=button], [contenteditable]";

/**
 * Lets an empty stretch of the top of the app move the window, the way a title bar does.
 * A double click maximizes or restores. Buttons and fields inside keep working.
 */
export function drag(node: HTMLElement) {
  if (!ownTitleBar) return;
  async function down(e: MouseEvent) {
    if (e.button !== 0 || (e.target as Element).closest(controls)) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    if (e.detail === 2) await getCurrentWindow().toggleMaximize();
    else await getCurrentWindow().startDragging();
  }
  node.addEventListener("mousedown", down);
  return { destroy: () => node.removeEventListener("mousedown", down) };
}

/** Closes this window: a popped-out viewer. A webview may ignore `window.close()`. */
export async function closeWindow() {
  if (!inTauri) return window.close();
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().close();
}
