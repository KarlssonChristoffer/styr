<!-- För: agent. One message you sent, in the chat: its files, the edit box, copy, edit and react (Transcript). -->
<script lang="ts">
  import { preview, THREAD_MIME } from "../attachments";
  import { here, putInBag } from "../bag.svelte";
  import type { Attachment } from "../bindings/Attachment";
  import { clock } from "../format";
  import { api } from "../api";
  import { hub } from "../state.svelte";
  import { openUrl } from "../native";
  import { copy, menu } from "../menu.svelte";
  import { askUndo, messageMenu } from "../menus.svelte";
  import type { Item as LogItem } from "../transcript";
  import { webAsked } from "../web";
  import { emojis, togglePicker } from "../reactions.svelte";
  import { plural, short } from "../transcriptWords";
  import Icon from "./Icon.svelte";
  import IconSwap from "./IconSwap.svelte";
  import Reactions from "./Reactions.svelte";
  import Receipt from "./Receipt.svelte";
  import RuleChip from "./RuleChip.svelte";
  import Switch from "./Switch.svelte";

  let { item, threadId, cwd, live = false }: { item: Extract<LogItem, { kind: "user" }>; threadId: string; cwd: string; live?: boolean } = $props();

  /** The text file sent with the message that is open under it, as its index. */
  let openFile = $state<number | null>(null);

  // Something sent along with a message can go into the context bag, for other threads.
  const bagMenu = (a: Attachment) => [{ label: "Put in the context bag", run: () => putInBag($state.snapshot(a) as Attachment, here(threadId, cwd)).catch((e) => hub.fail(e)) }];
  // Back to before a message, asked for in the panel where the files are listed.
  const undoTo = () => (item.checkpoint ? () => askUndo({ thread: threadId, to: item.checkpoint!, label: `“${short(item.text)}”` }) : undefined);
  // Copy and "Edit from here" on your own messages. Editing sends the new text in place of the
  // old one: what followed is left out of the conversation (and the files go back, if ticked).
  const mine = $derived(!item.fromHub && !item.review && !item.designReview && !item.sentBy && !item.terminal);
  let copied = $state(false);
  function copyMessage() {
    copy(item.text).then(() => {
      copied = true;
      setTimeout(() => (copied = false), 1200);
    });
  }
  let editing = $state<{ text: string; files: boolean } | null>(null);
  let sending = $state(false);
  const startEdit = () => (editing = { text: item.text, files: !!item.checkpoint });
  async function sendEdit() {
    if (!editing || sending) return;
    const { text, files } = editing;
    if (!text.trim() && !item.attachments.length) return;
    sending = true;
    try {
      await api.editFrom(threadId, item.seq, { text, attachments: item.attachments, files: files && !!item.checkpoint });
      editing = null;
    } catch (err) {
      hub.fail(err);
    } finally {
      sending = false;
    }
  }
  function editKey(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      sendEdit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      editing = null;
    }
  }
  function grow(el: HTMLTextAreaElement) {
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
    const fit = () => {
      el.style.height = "auto";
      el.style.height = Math.min(el.scrollHeight, window.innerHeight * 0.5) + "px";
    };
    fit();
    el.addEventListener("input", fit);
  }
</script>

<div class="user">
  {#if item.attachments.length}
    <div class="sent">
      {#each item.attachments as a, j (j)}
        {#if a.url}
          <button class="file link" data-tip={a.url} use:menu={() => bagMenu(a)} onclick={() => openUrl(a.url!).catch((e) => hub.fail(e))}><Icon name="link" size={12} />{a.name}</button>
        {:else if preview(a)}
          <img src={preview(a)} alt={a.name} data-tip={a.name} use:menu={() => bagMenu(a)} />
        {:else if a.mime === THREAD_MIME}
          {@const id = /\(id ([^)\s]+)\)/.exec(a.text ?? "")?.[1]}
          <button class="file thread" data-tip="Open the thread" disabled={!id || !hub.threads.some((t) => t.id === id)} onclick={() => id && hub.open(id)}><Icon name="threads" size={12} />{a.name}</button>
        {:else if a.text}
          {@const key = j}
          <button class="file" class:open={openFile === key} data-tip={openFile === key ? "Hide what it says" : "Show what it says"} onclick={() => (openFile = openFile === key ? null : key)}><Icon name="file" size={12} />{a.name}</button>
        {:else}
          <span class="file" data-tip={a.path ?? a.name} use:menu={() => bagMenu(a)}><Icon name={a.path && a.mime === "inode/directory" ? "folder" : "file"} size={12} />{a.name}</span>
        {/if}
      {/each}
    </div>
    {#if openFile !== null}
      {@const a = item.attachments[openFile!]}
      {#if a?.text}<pre class="file-text">{a.text}</pre>{/if}
    {/if}
  {/if}
  {#if editing}
    {@const ed = editing}
    <div class="editing">
      <textarea aria-label="Edit the message" bind:value={ed.text} onkeydown={editKey} use:grow></textarea>
      <div class="edit-row">
        {#if item.checkpoint}
          <label class="opt" data-tip="Roll the files back to how they were before this message. It can be undone like any rollback."><Switch label="Files too" on={ed.files} onchange={(on) => (ed.files = on)} /> Files too</label>
        {/if}
        {#if item.attachments.length}<span class="kept"><Icon name="clip" size={11} />{plural(item.attachments.length, "attachment")} kept</span>{/if}
        <span class="grow"></span>
        <button class="btn" onclick={() => (editing = null)}>Cancel</button>
        <button class="btn primary" disabled={sending || (!ed.text.trim() && !item.attachments.length)} data-tip={live ? "Stops the turn and sends this in place of the original" : "Sends this in place of the original; what came after is left out"} onclick={() => sendEdit()}>{sending ? "Sending…" : "Send"}</button>
      </div>
      <div class="edit-hint">Later messages are dropped.</div>
    </div>
  {:else if item.text}
    <!-- A web search shows your own words with a globe, never the instruction Hub sent. -->
    {@const asked = webAsked(item.text)}
    <div class="bubble" use:menu={() => messageMenu(asked ?? item.text, undoTo(), cwd, mine ? () => startEdit() : undefined)}>{#if asked !== null}<span class="webtag"><Icon name="globe" size={11} />Web</span>{/if}{asked ?? item.text}</div>
  {/if}
  {#if item.original}
    <details class="original"><summary>Tidied · what you wrote</summary><div class="bubble">{item.original}</div></details>
  {/if}
  <Reactions {threadId} seq={item.seq} />
  <span class="time">
    {#if item.terminal}<span class="from"><Icon name="terminal" size={11} /> in the terminal</span>{/if}
    {#if item.sentBy}
      {@const by = item.sentBy}
      {#if by.thread}
        <button class="from link" data-tip="Open the thread that sent this" onclick={() => hub.open(by.thread!)}><Icon name="hub" size={11} /> from {by.name}</button>
      {:else}
        <span class="from"><Icon name="hub" size={11} /> from {by.name}</span>
      {/if}
    {/if}
    {#if item.rule || item.saved}<RuleChip {threadId} seq={item.seq} rule={item.rule ?? ""} saved={item.saved} />{/if}
    {#if !editing}
      <span class="acts">
        {#if item.text}<button class="act" data-tip={copied ? "Copied" : "Copy the message"} aria-label="Copy the message" onclick={() => copyMessage()}><IconSwap name={copied ? "check" : "copy"} size={13} /></button>{/if}
        {#if emojis.on}<button class="act" data-react-btn data-tip="React" aria-label="React" onclick={(e) => togglePicker(e.currentTarget, threadId, item.seq)}><Icon name="smilePlus" size={13} /></button>{/if}
        {#if mine}<button class="act" data-tip="Edit from here: send it again changed, and leave out what came after" aria-label="Edit from here" onclick={() => startEdit()}><Icon name="pen" size={13} /></button>{/if}
      </span>
    {/if}
    <span data-tip={new Date(item.ts).toLocaleString([], { dateStyle: "medium", timeStyle: "short" })}>{clock(item.ts)}</span>
    {#if item.receipt}<Receipt sent={item.ts} receipt={item.receipt} />{/if}
  </span>
</div>
<style>
  /* @kit transcript */
</style>
