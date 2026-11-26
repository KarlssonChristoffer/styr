// Dictation in the message box (Composer.svelte). Ctrl D or the mic toggles it. Speech is turned
// into text by whisper.cpp in the core, so it works in the background without any Windows panel.
// While the user talks, the current phrase is transcribed again and again and written straight
// into the box at the caret; a pause ends the phrase and its better, final text replaces the
// guess in place. Enter while dictating waits for the last words before it sends.
// The box itself stays with the composer: this class sees it only through `ComposerBox`.
import { tick } from "svelte";
import { Dictation, Steady } from "./dictation";
import { quiet, quietly } from "./quiet";
import { chirp, microphones, Recorder, voiceApi } from "./voice";

export type VoiceState = "idle" | "recording" | "transcribing";

/** What dictation needs of the message box. */
export type ComposerBox = {
  text: string;
  /** Where the caret is; dictation moves it itself after it writes. */
  readonly caret: number;
  /** The textarea, to keep the focus in it while dictating. */
  element: () => HTMLTextAreaElement | undefined;
  /** The thread's folder, which gives the transcriber its hints. */
  cwd: () => string | undefined;
  onerror: (e: unknown) => void;
};

const MIC_KEY = "hub.voice.mic";
const MUTE_KEY = "hub.voice.muteChat";
// Long enough not to split a sentence at a breath; whisper does better with the whole thing.
const PAUSE_S = 1.2;
const LONGEST_S = 25;
/** How often to redo the live guess, at most. */
const GUESS_MS = 500;
const TIDYING = "Tidying what you said…";

function readMic() {
  try {
    return localStorage.getItem(MIC_KEY) ?? "";
  } catch {
    return "";
  }
}

function readMuteChat() {
  try {
    return localStorage.getItem(MUTE_KEY) !== "off";
  } catch {
    return true;
  }
}

function dictatedHere() {
  try {
    return localStorage.getItem("hub.voice.used") === "1";
  } catch {
    return false;
  }
}

export class ComposerVoice {
  state = $state<VoiceState>("idle");
  note = $state("");
  bars = $state([0, 0, 0, 0, 0]);
  mic = $state(readMic());
  mics = $state<{ id: string; name: string }[]>([]);
  /** Whether dictating mutes the voice chat; the key for it is set up in Settings. */
  muteChat = $state(readMuteChat());
  muteKey = $state<string | null>(null);
  /** Bumped when dictation lets go of its stretches without changing the text. */
  private rev = $state(0);

  private dictation: Dictation;
  private recorder = new Recorder();
  /** Bumped when dictation is dropped, so phrases still being transcribed stay out of the box. */
  private take = 0;
  private segStart = 0;
  private speechAt = -1; // first loud sample of the phrase, -1 before any
  private lastLoud = 0;
  private floor = 0.004;
  private phrase = 0;
  private finals: Promise<void> = Promise.resolve();
  private muteTimer: ReturnType<typeof setInterval> | undefined;
  /** The microphone is opening; a second press meanwhile would start a second take. */
  private starting = false;
  private frame = 0;
  private warmedAt = 0;
  /** The box before dictation started, to find what was said when tidying it at the end. */
  private textAtStart = "";
  /** Settings > Dictation asks for the small model to tidy dictated text. */
  private polishOn = false;

  constructor(private box: ComposerBox) {
    this.dictation = new Dictation(this.field);
  }

  /** The box as dictation sees it: the text and the caret. */
  private field = {
    get text() {
      return this.owner.box.text;
    },
    set text(v: string) {
      this.owner.box.text = v;
    },
    get caret() {
      return this.owner.box.caret;
    },
    set caret(at: number) {
      const el = this.owner.box.element();
      if (el && document.activeElement === el) {
        // After the box shows the new text, which would put the caret at the end.
        tick().then(() => {
          if (el && document.activeElement === el) el.selectionStart = el.selectionEnd = at;
        });
      }
    },
    owner: this as ComposerVoice,
  };

  /**
   * The text in pieces, with the words still being heard marked: they are drawn grey, as in
   * Claude Code, until their final text is in. A textarea cannot colour part of its text, so
   * while there are any, a copy of the text is drawn under it and the textarea's own letters are
   * made transparent (the caret and selection stay).
   */
  pending(text: string) {
    void this.rev;
    const ranges = this.state === "idle" ? [] : this.dictation.ranges();
    if (!ranges.length) return null;
    const parts: { text: string; grey: boolean }[] = [];
    let at = 0;
    for (const [from, to] of ranges.sort((a, b) => a[0] - b[0])) {
      if (from > at) parts.push({ text: text.slice(at, from), grey: false });
      parts.push({ text: text.slice(Math.max(from, at), to), grey: true });
      at = Math.max(at, to);
    }
    parts.push({ text: text.slice(at) + "​", grey: false });
    return parts;
  }

  toggleMuteChat() {
    this.muteChat = !this.muteChat;
    try {
      localStorage.setItem(MUTE_KEY, this.muteChat ? "on" : "off");
    } catch {
      // Then it is only for this session.
    }
    if (this.state === "recording") {
      if (this.muteChat) this.muteNow();
      else this.unmute();
    }
  }

  /** Fills the microphone menu when it opens. */
  async loadMics() {
    quiet(voiceApi.status().then((s) => (this.muteKey = s.mute_key)), "status");
    this.mics = await quiet(microphones(), "microphones", []);
  }

  async pickMic(id: string) {
    this.mic = id;
    try {
      localStorage.setItem(MIC_KEY, id);
    } catch {
      // Then it is only for this session.
    }
    // Switch at once when it is already listening.
    if (this.state === "recording") {
      await this.finish();
      await this.toggle();
    }
  }

  private heard = (rms: number, at: number) => {
    const rate = this.recorder.rate;
    const loud = rms > Math.max(0.012, this.floor * 3);
    if (loud) {
      if (this.speechAt < 0) {
        this.speechAt = at;
        this.dictation.open(this.phrase);
      }
      this.lastLoud = at;
    } else {
      this.floor = this.floor * 0.95 + rms * 0.05;
    }
    if (this.speechAt >= 0 && (at - this.lastLoud > PAUSE_S * rate || at - this.speechAt > LONGEST_S * rate)) this.endPhrase(at);
  };

  private phraseStart() {
    return Math.max(this.segStart, this.speechAt - Math.round(0.3 * this.recorder.rate));
  }

  /** Hands the phrase so far to the transcriber for its final text, in order. */
  private endPhrase(at: number) {
    if (this.speechAt < 0) return;
    const audio = this.recorder.slice(this.phraseStart(), at);
    this.segStart = at;
    this.speechAt = -1;
    const id = this.phrase++;
    const take = this.take;
    this.finals = this.finals.then(async () => {
      if (take !== this.take || !this.dictation.has(id)) return;
      try {
        const said = await voiceApi.transcribe(audio, this.dictation.before(id), true, this.box.cwd());
        if (take === this.take) this.dictation.final(id, said);
      } catch (e) {
        // The live guess stays in the box; better than nothing.
        if (take === this.take) this.dictation.final(id, "");
        this.box.onerror(e);
      }
    });
  }

  /** Redoes the live guess for the phrase being spoken, back to back, as fast as whisper goes. */
  private async guessLoop(take: number) {
    let steady = { phrase: -1, words: new Steady() };
    while (take === this.take && this.state === "recording") {
      const started = performance.now();
      const mine = this.phrase;
      if (steady.phrase !== mine) steady = { phrase: mine, words: new Steady() };
      if (this.speechAt >= 0 && this.dictation.has(mine)) {
        try {
          const said = await voiceApi.transcribe(this.recorder.slice(this.phraseStart()), this.dictation.before(mine), false, this.box.cwd());
          // The phrase may have ended meanwhile; its guess still shows until the final text.
          if (take === this.take && said) this.dictation.live(mine, steady.words.next(said));
        } catch {
          // A missed guess is fine; the final text follows at the pause.
        }
      }
      await new Promise((r) => setTimeout(r, Math.max(150, GUESS_MS - (performance.now() - started))));
    }
  }

  private animate = () => {
    this.bars = [...this.bars.slice(1), this.recorder.level()];
    this.frame = requestAnimationFrame(() => setTimeout(this.animate, 60));
  };

  /**
   * Loads the speech model when the box gets focus, so it is ready by the time Ctrl D is pressed:
   * a cold start (load plus the graphics card's first decode) took 5 to 7 seconds before the first
   * live words. Only when dictation was used in this window, and at most once a minute; the core
   * lets the model go again after 10 minutes without use.
   */
  warm() {
    if (!dictatedHere() || Date.now() - this.warmedAt < 60_000) return;
    this.warmedAt = Date.now();
    voiceApi.prepare();
  }

  async toggle() {
    if (this.state === "transcribing") return;
    if (this.state === "recording") return this.finish(true);
    if (this.starting) return;
    this.segStart = 0;
    this.speechAt = -1;
    this.floor = 0.004;
    // Load the model now, while the microphone opens, so the first words do not wait for it.
    // The first use downloads it, and a graphics card gets a faster build and a bigger model in
    // the background later.
    voiceApi.prepare();
    voiceApi
      .status()
      .then((s) => {
        this.polishOn = s.polish;
        if (s.state !== "ready") this.note = "Setting up voice the first time (downloads about 200 MB)…";
      })
      .catch(quietly("voice status"));
    this.recorder.onblock = this.heard;
    this.starting = true;
    try {
      await this.recorder.start(this.mic);
    } catch (e) {
      this.box.onerror(new Error(`Microphone unavailable: ${e instanceof Error ? e.message : e}`));
      return;
    } finally {
      this.starting = false;
    }
    this.dictation = new Dictation(this.field);
    this.rev++;
    this.textAtStart = this.box.text;
    this.state = "recording";
    this.warmedAt = Date.now();
    try {
      localStorage.setItem("hub.voice.used", "1");
    } catch {
      // Then the box does not warm the model up ahead of time.
    }
    chirp(true);
    const el = this.box.element();
    if (!el || document.activeElement !== el) el?.focus();
    // Mutes the voice chat (Discord, say) while dictating, when a mute key is set up.
    if (this.muteChat) this.muteNow();
    this.guessLoop(this.take);
    this.animate();
  }

  /**
   * Mute calls go out one after the other. Sent loose, a slow "hold" could land after the "let go"
   * and keep the chat muted for the core's whole 30 s lease, with nobody dictating.
   */
  private muteChain: Promise<unknown> = Promise.resolve();

  private sendMute(on: boolean) {
    this.muteChain = this.muteChain.then(() => voiceApi.mute(on));
  }

  private muteNow() {
    clearInterval(this.muteTimer);
    this.sendMute(true);
    // Only while still recording: a tick that outlives dictation would hold the key again.
    this.muteTimer = setInterval(() => {
      if (this.state === "recording" && this.muteChat) this.sendMute(true);
      else clearInterval(this.muteTimer);
    }, 10_000);
  }

  private unmute() {
    clearInterval(this.muteTimer);
    this.sendMute(false);
  }

  /** Stops listening and waits for the last words; `polish` then tidies them when that is on. */
  async finish(polish = false) {
    if (this.state !== "recording") return this.finals;
    chirp(false);
    this.unmute();
    cancelAnimationFrame(this.frame);
    this.recorder.onblock = null;
    this.endPhrase(this.recorder.length);
    this.state = "transcribing";
    await this.recorder.stop();
    this.bars = [0, 0, 0, 0, 0];
    await this.finals;
    this.state = "idle";
    this.note = "";
    this.dictation.release();
    this.rev++;
    if (polish && this.polishOn) await this.polishDictated(this.textAtStart);
  }

  /**
   * Has the small model tidy what was dictated (all that changed in the box since `before`),
   * and puts the tidy text in its place unless the box changed meanwhile, as by sending.
   */
  private async polishDictated(before: string) {
    const now = this.box.text;
    let a = 0;
    while (a < before.length && a < now.length && before[a] === now[a]) a++;
    let b = 0;
    while (b < before.length - a && b < now.length - a && before[before.length - 1 - b] === now[now.length - 1 - b]) b++;
    const said = now.slice(a, now.length - b);
    if (said.trim().split(/\s+/).length < 3) return;
    this.note = TIDYING;
    try {
      const tidy = (await voiceApi.polish(said, this.box.cwd())).trim();
      if (this.box.text === now && tidy) {
        const lead = said.match(/^\s*/)?.[0] ?? "";
        const tail = said.match(/\s*$/)?.[0] ?? "";
        this.box.text = now.slice(0, a) + lead + tidy + tail + now.slice(now.length - b);
      }
    } catch (e) {
      this.box.onerror(e);
    } finally {
      if (this.note === TIDYING) this.note = "";
    }
  }

  /**
   * Before sending while dictating: stops listening and waits for the last words' final text, so
   * what was just said goes with the message. A slow transcriber gets three seconds; after that
   * the live guess already in the box is sent as it is.
   */
  async settle() {
    if (this.state === "idle") return;
    const done = this.finish();
    const late = await Promise.race([done.then(() => false), new Promise<boolean>((r) => setTimeout(() => r(true), 3000))]);
    if (late) await this.drop();
  }

  /** Stops dictation without waiting for the last phrase, as after sending. */
  async drop() {
    if (this.state === "idle") return;
    this.take++;
    if (this.state === "recording") chirp(false);
    this.unmute();
    cancelAnimationFrame(this.frame);
    this.recorder.onblock = null;
    this.speechAt = -1;
    this.phrase++;
    await this.recorder.stop();
    this.bars = [0, 0, 0, 0, 0];
    this.finals = Promise.resolve();
    this.state = "idle";
    this.note = "";
    this.dictation.release();
    this.rev++;
  }

  /** The box goes away: nothing more may land in it, and the microphone closes. */
  destroy() {
    this.take++;
    // The timer outlives the box otherwise and keeps Discord muted every 10 s.
    this.unmute();
    cancelAnimationFrame(this.frame);
    if (this.recorder.active) this.recorder.stop();
  }
}
