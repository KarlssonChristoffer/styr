// Opening, adding and removing projects, and the library threads that teach a note. The calls live on `hub`; these hold what they do.
import { api, type NewProjectMore } from "./api";
import type { NewProject } from "./bindings/NewProject";
import { rememberFolder } from "./threadFlags";
import type { HubState } from "./state.svelte";

/** Adds or creates a project and opens its page. `host`: the machine it is made on, when not this one.
 * Returns what did not work out, and the setup thread when one started. */
export async function addProject(hub: HubState, p: NewProject & NewProjectMore, host: string | null = null): Promise<{ notes: string[]; path: string; thread: string | null }> {
  const created = await api.createProject(p, host);
  const project = created.project;
  if (p.create) hub.firstRun = { path: project.path, host: host && host !== hub.localHost ? host : null, thread: created.thread_id ?? null };
  if (host) {
    hub.openProject(project.path, host);
  } else {
    if (!hub.projects.some((x) => x.path === project.path)) hub.projects.unshift(project);
    rememberFolder(project.path);
    hub.openProject(project.path);
  }
  return { notes: created.notes, path: project.path, thread: created.thread_id };
}

/** Opens New project for a quick thread: its title is the name, and the setup thread is told where its files are. */
export function makeProjectOf(hub: HubState, t: { title: string; cwd: string; host?: string | null }) {
  const where = t.host && t.host !== hub.localHost ? ` on ${t.host}` : "";
  hub.newProjectFrom = {
    name: t.title.replace(/[^\p{L}\p{N} _-]/gu, "").trim().slice(0, 40),
    about: `This started as a quick thread, "${t.title}". Its files are in ${t.cwd}${where}. Bring over what belongs to the project and leave the rest.`,
    from: t.cwd,
    host: t.host ?? null,
  };
  hub.newProjectOpen = true;
}

export async function removeProject(hub: HubState, path: string) {
  try {
    await api.removeProject(path);
    hub.projects = hub.projects.filter((p) => p.path !== path);
    if (!hub.threads.some((t) => t.cwd === path)) hub.goHome();
  } catch (e) {
    hub.fail(e);
  }
}

/** Opens a project's page; `host` is the machine its folder is on, when not this one. */
export function openProject(hub: HubState, cwd: string, host: string | null = null) {
  const remote = host && host !== hub.localHost ? host : null;
  // A project page opens on its overview, unless you are on it already.
  if (hub.view !== "project" || hub.project !== cwd || hub.projectHost !== remote) {
    hub.projectTab = "overview";
    hub.projectDoc = null;
    hub.projectItem = null;
  }
  if (hub.project !== cwd || hub.projectHost !== remote) hub.github = { mode: "pulls", pull: null, issue: null };
  hub.view = "project";
  hub.project = cwd;
  hub.projectHost = remote;
  hub.selected = null;
}

/** Starts a thread in the library that teaches one note and checks it stuck. */
export async function learn(hub: HubState, note: string) {
  try {
    const [{ root }, { message }] = await Promise.all([api.library(), api.libraryPrompt({ learn: note })]);
    await hub.newThread(root, message);
  } catch (e) {
    hub.fail(e);
  }
}

/** Asks the library a question in a new thread, which looks it up and writes back what it learns. */
export async function askLibrary(hub: HubState, question: string) {
  try {
    const [{ root }, { message }] = await Promise.all([api.library(), api.libraryPrompt({ ask: question })]);
    await hub.newThread(root, message);
  } catch (e) {
    hub.fail(e);
  }
}
