// The context bag: files, pictures, links and snippets kept apart from any thread, to take along
// into whichever thread needs them, in any project. The core keeps them (`bag.rs`), so the bag is
// the same on every window and on the phone; the list comes without content, and an item's
// content is fetched when it goes into a message or the viewer.
import { quiet } from "./quiet";
import { api } from "./api";
import type { Attachment } from "./bindings/Attachment";
import type { BagItem } from "./bindings/BagItem";
import { dropped } from "./drafts.svelte";
import { fromFile } from "./attachments";
import { fold } from "./transcript";
import { ago } from "./format";
import { hub } from "./state.svelte";

/** What a bag item carries while it is dragged onto a thread's row: its id. */
export const BAG_DRAG = "application/x-hub-bag";
export const LINK_MIME = "text/uri-list";

export const bag = $state<{ items: BagItem[]; loaded: boolean }>({ items: [], loaded: false });

/** The drop zone that shows anywhere in the window while something is dragged: `thread` is a
 *  thread being dragged from the list, `over` that the pointer is on the zone. */
export const bagDrop = $state<{ show: boolean; thread: string | null; over: boolean }>({ show: false, thread: null, over: false });

/** Whether a drag carries something the bag takes: files, links, pictures or text. */
export function bagTakes(dt: DataTransfer | null): boolean {
  if (!dt || dt.types.includes(BAG_DRAG)) return false;
  return ["Files", "text/uri-list", "text/html", "text/plain"].some((t) => dt.types.includes(t));
}

const IMAGE_URL = /\.(png|jpe?g|gif|webp|avif|bmp|svg)(\?.*)?$/i;

/** A picture by its address: a data: address is read here, a web one fetched by the core. Falls
 *  back to a link when it is not a picture after all. */
async function picture(src: string): Promise<Attachment | null> {
  const data = /^data:(image\/[\w.+-]+);base64,(.+)$/i.exec(src);
  if (data) return { name: `picture.${data[1].split("/")[1].replace("jpeg", "jpg")}`, mime: data[1], data: data[2] };
  if (!/^https?:\/\//i.test(src)) return null;
  try {
    return await api.fetchPicture(src);
  } catch {
    return null;
  }
}

/** Puts whatever was dropped or pasted in the bag: files and screenshots as they are, a picture
 *  dragged out of a web page as the picture, a link as a link and other text as a note. Says how
 *  many went in. */
export async function putDropped(dt: DataTransfer, from?: string): Promise<number> {
  const files = [...dt.files];
  // Read everything before the first await; the browser empties a DataTransfer after the event.
  const html = dt.getData("text/html");
  const uris = dt.getData("text/uri-list");
  const plain = dt.getData("text/plain");
  let n = 0;
  const put = async (a: Attachment | null) => {
    if (!a) return;
    try {
      await putInBag(a, from);
      n++;
    } catch (e) {
      hub.fail(e);
    }
  };
  if (files.length) {
    for (const f of files) await put(await fromFile(f).catch((e) => (hub.fail(e), null)));
    return n;
  }
  const img = html && /<img[^>]+src\s*=\s*["']([^"']+)["']/i.exec(html)?.[1];
  const url = uris.split(/\r?\n/).find((u) => u && !u.startsWith("#"))?.trim() || plain.trim();
  if (img || (url && IMAGE_URL.test(url))) {
    const pic = await picture((img || url).replace(/&amp;/g, "&"));
    if (pic) {
      await put(pic);
      return n;
    }
  }
  const link = url ? asLink(url) : null;
  if (link) await put(link);
  else if (plain.trim()) await put(note(plain));
  return n;
}

let notes = 0;
/** Text kept as a short note, named by its first line. */
export function note(text: string): Attachment {
  const value = text.trim();
  notes++;
  return { name: `${value.split("\n")[0].slice(0, 40).trim() || "note"}${notes > 1 ? ` ${notes}` : ""}.md`, mime: "text/markdown", text: value };
}

/** A thread in the bag: its title, folder and the conversation so far, as a note another thread
 *  can read. Long threads keep their latest messages. */
export async function threadInBag(id: string, from?: string) {
  const t = hub.threads.find((x) => x.id === id);
  if (!t) return;
  try {
    const said = fold(await api.events(id)).flatMap((i) =>
      i.kind === "user" ? [`**You:** ${i.text.trim()}`] : i.kind === "assistant" && i.text.trim() ? [`**${t.engine}:** ${i.text.trim()}`] : [],
    );
    const LIMIT = 40_000;
    let body = "";
    for (let i = said.length - 1; i >= 0 && body.length + said[i].length < LIMIT; i--) body = `${said[i]}\n\n${body}`;
    const cut = body.length < said.join("\n\n").length;
    const text = [
      `# Thread: ${t.title}`,
      "",
      `Folder: ${t.cwd}${t.host ? ` on ${t.host}` : ""} · engine: ${t.engine} · Hub thread id: ${t.id}`,
      "",
      cut ? "(Earlier messages left out.)\n" : "",
      body.trim() || "(Nothing said yet.)",
    ].join("\n");
    await putInBag({ name: `Thread · ${t.title.slice(0, 50)}.md`, mime: "text/markdown", text }, from);
  } catch (e) {
    hub.fail(e);
  }
}

export async function loadBag() {
  try {
    bag.items = await api.bag();
    bag.loaded = true;
  } catch (e) {
    hub.fail(e);
  }
}

/** Puts an attachment in the bag; pictures get a small copy for the list. */
export async function putInBag(item: Attachment, from?: string) {
  const thumb = item.data && item.mime.startsWith("image/") ? await quiet(thumbnail(`data:${item.mime};base64,${item.data}`), "thumbnail") : undefined;
  const added = await api.putInBag({ item, thumb, from });
  bag.items = [added, ...bag.items.filter((b) => b.id !== added.id)];
  hub.tell(`${added.item.name} is in the bag`);
  return added;
}

/** A web address, named by itself until there is something better. */
export function asLink(text: string): Attachment | null {
  const url = text.trim();
  if (!/^https?:\/\/\S+$/i.test(url)) return null;
  let name = url;
  try {
    const u = new URL(url);
    name = u.hostname.replace(/^www\./, "") + (u.pathname !== "/" ? u.pathname.replace(/\/$/, "") : "");
  } catch {
    // Kept whole.
  }
  return { name, mime: LINK_MIME, url };
}

export async function takeFromBag(id: string) {
  await api.takeFromBag(id);
  bag.items = bag.items.filter((b) => b.id !== id);
}

export async function emptyBag() {
  await api.emptyBag();
  bag.items = [];
}

/** An item as it goes into a message, content and all. */
export async function fromBag(id: string): Promise<Attachment> {
  const listed = bag.items.find((b) => b.id === id);
  if (listed && (listed.item.url || listed.item.path)) return listed.item;
  return (await api.bagItem(id)).item;
}

/** Hands items to a thread's composer, the same way as files dropped on its row, and opens it. */
export async function intoThread(thread: string, ids: string[]) {
  try {
    const items = await Promise.all(ids.map(fromBag));
    dropped[thread] = [...(dropped[thread] ?? []), ...items];
    hub.open(thread);
  } catch (e) {
    hub.fail(e);
  }
}

/** Where something is put in the bag from: the open thread's title, or the folder's name. */
export function here(threadId?: string | null, cwd?: string): string | undefined {
  const t = threadId ? hub.threads.find((x) => x.id === threadId) : undefined;
  return t?.title || cwd?.split(/[\\/]/).filter(Boolean).pop() || undefined;
}

/** When it went in, for the list. */
export function when(b: BagItem): string {
  const a = ago(b.added_at);
  return a === "now" ? "just now" : `${a} ago`;
}

export function bagSize(b: BagItem): string {
  const bytes = b.size;
  if (!bytes) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function bagIcon(b: BagItem): "link" | "folder" | "image" | "file" {
  if (b.item.url) return "link";
  if (b.item.mime === "inode/directory") return "folder";
  if (b.item.mime.startsWith("image/")) return "image";
  return "file";
}

async function thumbnail(src: string, edge = 96): Promise<string> {
  const img = new Image();
  img.src = src;
  await img.decode();
  const scale = Math.min(1, edge / Math.max(img.naturalWidth, img.naturalHeight));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(img.naturalWidth * scale));
  canvas.height = Math.max(1, Math.round(img.naturalHeight * scale));
  canvas.getContext("2d")!.drawImage(img, 0, 0, canvas.width, canvas.height);
  return canvas.toDataURL("image/jpeg", 0.8);
}
