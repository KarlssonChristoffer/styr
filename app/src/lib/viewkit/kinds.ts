// Viewkit: the one place that says what kinds of output Hub shows and how they are named. A kind is
// something an agent made that a person may want to look at. Each is shown in three ways: a card
// in the chat (`Card.svelte`), big over the window (`Big.svelte`) and a row in a library
// (`Row.svelte`). Pages, pictures and files are the first kinds; a new kind is one entry here and
// the component that draws it. See "Viewkit" in docs/ARCHITECTURE.md.
import type { IconName } from "../components/Icon.svelte";

export type ViewKind = "page" | "picture" | "doc" | "file";

type Kind = {
  /** The grey word on a card's header and a row. */
  label: string;
  icon: IconName;
};

export const KINDS: Record<ViewKind, Kind> = {
  page: { label: "Page", icon: "page" },
  picture: { label: "Picture", icon: "image" },
  doc: { label: "Doc", icon: "file" },
  file: { label: "File", icon: "file" },
};

/** A kind from a library entry (`Output.kind`), which may be one this build does not know: it is a file then. */
export function kindOf(name: string): ViewKind {
  return name in KINDS ? (name as ViewKind) : "file";
}
