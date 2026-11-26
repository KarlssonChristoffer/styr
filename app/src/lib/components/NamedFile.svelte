<!-- A file a reply names (a table, a drawing, a page, a spreadsheet) that the agent made some other way than by writing it with an edit, such as a script: the card shows it once the folder is known to have it. A name that is no file (or none on that machine) shows nothing.
     Used by Changes.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  // A file a reply names (a table, a drawing, a page, a spreadsheet) that the agent made some other
  // way than by writing it with an edit, such as a script: the card shows it once the folder is
  // known to have it. A name that is no file (or none on that machine) shows nothing.
  import { api } from "../api";
  import { hub } from "../state.svelte";
  import { dirHints, fileKind, findFile, placeOf } from "../filePreview";
  import { fileMenu } from "../menus.svelte";
  import { grown, showFile, toggleGrown, unfolded, unfoldKey } from "../viewkit/openfile.svelte";
  import Card from "../viewkit/Card.svelte";
  import FilePreview from "./FilePreview.svelte";

  let { path, cwd, threadId, host = null, answer = "", compact = false }: { path: string; cwd: string; threadId?: string; host?: string | null; answer?: string; compact?: boolean } = $props();

  // The folder a script wrote to may be named in the answer without being in the file's own name.
  let found = $state<string | null>(null);
  const at0 = $derived(placeOf(path, cwd));
  const at = $derived(found !== null && at0.root === cwd ? { root: cwd, rel: found } : at0);
  $effect(() => {
    const p = at0;
    found = null;
    if (p.root !== cwd || p.rel.includes("/")) return;
    let gone = false;
    quiet(findFile((d) => api.projectTree(p.root, d, host), p.rel, dirHints(answer)).then((r) => !gone && (found = r)), "find file");
    return () => (gone = true);
  });
  const name = $derived(at.rel.replace(/\\/g, "/"));
  // The big view and the side pane open what the card shows: the place found, not the bare name.
  const opens = $derived(at.root === cwd ? at.rel : path);
  let there = $state(false);
  $effect(() => {
    const where = { root: at.root, rel: at.rel, host };
    there = false;
    let gone = false;
    const k = fileKind(where.rel);
    const probe = k === "sheet" ? api.projectSheet(where.root, where.rel, where.host) : k === "database" ? api.projectDatabase(where.root, where.rel, where.host) : api.projectFile(where.root, where.rel, where.host);
    quiet(probe.then(() => !gone && (there = true)), "then");
    return () => (gone = true);
  });
</script>

{#if there}
  <Card
    kind="file"
    {name}
    {compact}
    onexpand={threadId ? () => showFile(threadId, opens, null, { card: !compact }) : undefined}
    onpanel={threadId ? () => showFile(threadId, opens, null, { how: "panel" }) : undefined}
    onmodal={threadId ? () => showFile(threadId, opens, null, { how: "modal" }) : undefined}
    unfolded={!!threadId && !!unfolded[unfoldKey(threadId, opens)]}
    flat={fileKind(at.rel) === "html"}
    grown={!!threadId && !!grown[unfoldKey(threadId, opens)]}
    ongrow={threadId ? () => toggleGrown(threadId, opens) : undefined}
    items={() => fileMenu(path, { rel: name, remote: !!host })}
  >
    {#key path}<FilePreview tile root={at.root} rel={at.rel} {host} />{/key}
    {#snippet full()}<FilePreview flat root={at.root} rel={at.rel} {host} />{/snippet}
  </Card>
{/if}
