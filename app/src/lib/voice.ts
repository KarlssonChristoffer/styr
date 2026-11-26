// Records the microphone as 16 kHz mono WAV for the core's whisper.cpp, and plays the short
// chirps that mark the start and end of a recording.
import { quiet } from "./quiet";
import { BASE } from "./api";
import type { Transcript } from "./bindings/Transcript";
import type { VoiceStatus } from "./bindings/VoiceStatus";

const RATE = 16_000;

export class Recorder {
  private ctx: AudioContext | null = null;
  private stream: MediaStream | null = null;
  private node: ScriptProcessorNode | null = null;
  private analyser: AnalyserNode | null = null;
  private chunks: Float32Array[] = [];
  private total = 0;
  private wave = new Uint8Array(512);
  /** Called with each new block of samples and its loudness, for voice activity detection. */
  onblock: ((rms: number, at: number) => void) | null = null;

  /** `device` is a microphone's id from `microphones()`; empty is the system default. */
  async start(device = "") {
    // The browser's noise suppression smears consonants, which costs whisper more than the
    // noise it removes.
    const audio = { channelCount: 1, echoCancellation: true, noiseSuppression: false, autoGainControl: true };
    try {
      this.stream = await navigator.mediaDevices.getUserMedia({ audio: device ? { ...audio, deviceId: { exact: device } } : audio });
    } catch (e) {
      // The chosen one is unplugged: the default beats no dictation.
      if (!device) throw e;
      this.stream = await navigator.mediaDevices.getUserMedia({ audio });
    }
    // The browser resamples the microphone to the context's rate.
    this.ctx = new AudioContext({ sampleRate: RATE });
    const source = this.ctx.createMediaStreamSource(this.stream);
    this.analyser = this.ctx.createAnalyser();
    this.analyser.fftSize = 512;
    source.connect(this.analyser);
    // ScriptProcessor is deprecated but needs no worklet file, which the app's CSP would block.
    this.node = this.ctx.createScriptProcessor(2048, 1, 1);
    this.chunks = [];
    this.total = 0;
    this.node.onaudioprocess = (e) => {
      const block = new Float32Array(e.inputBuffer.getChannelData(0));
      this.chunks.push(block);
      this.total += block.length;
      let sum = 0;
      for (const v of block) sum += v * v;
      this.onblock?.(Math.sqrt(sum / block.length), this.total);
    };
    source.connect(this.node);
    this.node.connect(this.ctx.destination);
  }

  /** Samples recorded so far. */
  get length() {
    return this.total;
  }

  get rate() {
    return this.ctx?.sampleRate ?? RATE;
  }

  /** Loudness right now, 0 to about 1, for the waveform. */
  level(): number {
    if (!this.analyser) return 0;
    this.analyser.getByteTimeDomainData(this.wave);
    let sum = 0;
    for (const v of this.wave) sum += ((v - 128) / 128) ** 2;
    return Math.min(1, Math.sqrt(sum / this.wave.length) * 4);
  }

  /** The recording between two sample positions, as WAV. */
  slice(from: number, to = this.total): Blob {
    const parts: Float32Array[] = [];
    let at = 0;
    for (const c of this.chunks) {
      const start = Math.max(from - at, 0);
      const end = Math.min(to - at, c.length);
      if (end > start) parts.push(c.subarray(start, end));
      at += c.length;
      if (at >= to) break;
    }
    return wav(parts, parts.reduce((n, p) => n + p.length, 0), this.rate);
  }

  async stop() {
    this.node?.disconnect();
    this.analyser?.disconnect();
    this.stream?.getTracks().forEach((t) => t.stop());
    await this.ctx?.close();
    this.ctx = null;
    this.analyser = null;
  }

  get active() {
    return this.ctx !== null;
  }
}

function wav(chunks: Float32Array[], length: number, rate: number): Blob {
  const buf = new DataView(new ArrayBuffer(44 + length * 2));
  const text = (at: number, s: string) => [...s].forEach((c, i) => buf.setUint8(at + i, c.charCodeAt(0)));
  text(0, "RIFF");
  buf.setUint32(4, 36 + length * 2, true);
  text(8, "WAVE");
  text(12, "fmt ");
  buf.setUint32(16, 16, true);
  buf.setUint16(20, 1, true); // PCM
  buf.setUint16(22, 1, true); // mono
  buf.setUint32(24, rate, true);
  buf.setUint32(28, rate * 2, true);
  buf.setUint16(32, 2, true);
  buf.setUint16(34, 16, true);
  text(36, "data");
  buf.setUint32(40, length * 2, true);
  let at = 44;
  for (const chunk of chunks) {
    for (const s of chunk) {
      buf.setInt16(at, Math.max(-1, Math.min(1, s)) * 0x7fff, true);
      at += 2;
    }
  }
  return new Blob([buf], { type: "audio/wav" });
}

/** The microphones to pick from. Their names show once the app may use the microphone. */
export async function microphones(): Promise<{ id: string; name: string }[]> {
  const all = await navigator.mediaDevices.enumerateDevices();
  return all
    .filter((d) => d.kind === "audioinput" && d.deviceId !== "default" && d.deviceId !== "communications")
    .map((d, i) => ({ id: d.deviceId, name: d.label || `Microphone ${i + 1}` }));
}

/** A short rising chirp when recording starts, a falling one when it stops. */
export function chirp(up: boolean) {
  try {
    const ctx = new AudioContext();
    const t = ctx.currentTime;
    const notes = up ? [660, 990] : [990, 660];
    notes.forEach((f, i) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = "sine";
      osc.frequency.value = f;
      const start = t + i * 0.08;
      gain.gain.setValueAtTime(0, start);
      gain.gain.linearRampToValueAtTime(0.12, start + 0.01);
      gain.gain.exponentialRampToValueAtTime(0.001, start + 0.09);
      osc.connect(gain).connect(ctx.destination);
      osc.start(start);
      osc.stop(start + 0.1);
    });
    setTimeout(() => ctx.close(), 400);
  } catch {
    // Sound is only a cue.
  }
}

export const voiceApi = {
  /** Holds the voice chat's mute key while dictating; send `true` again every few seconds. */
  mute: (on: boolean) => quiet(fetch(`${BASE}/api/voice/mute?on=${on}`, { method: "POST", keepalive: true }), "mute"),
  /** Presses `key` once after three seconds, for the chat app's keybind recorder. */
  teach: (key: string) => fetch(`${BASE}/api/voice/mute/teach?key=${encodeURIComponent(key)}`, { method: "POST" }),
  status: async (): Promise<VoiceStatus> => (await fetch(BASE + "/api/voice")).json(),
  prepare: () => fetch(BASE + "/api/voice/prepare", { method: "POST" }),
  /**
   * `before` is the text the words follow; `final` asks for the slower, better decode. `cwd` is
   * the thread's folder, whose name, branch and changed files help whisper spell them.
   */
  transcribe: async (audio: Blob, before = "", final = false, cwd?: string): Promise<string> => {
    const query = new URLSearchParams({ prompt: before.slice(-400), final: String(final) });
    if (cwd) query.set("cwd", cwd);
    // A stuck transcriber must not leave the mic spinning for good.
    const res = await fetch(`${BASE}/api/voice/transcribe?${query}`, { method: "POST", body: audio, signal: AbortSignal.timeout(90_000) });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error(`Voice: ${err.error ?? res.statusText}`);
    }
    return ((await res.json()) as Transcript).text;
  },
  /** Dictated text tidied by the small model (punctuation, tech words, no "eh"), same wording. */
  polish: async (text: string, cwd?: string): Promise<string> => {
    const res = await fetch(`${BASE}/api/voice/polish`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text, cwd }),
      signal: AbortSignal.timeout(35_000),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error(`Voice: ${err.error ?? res.statusText}`);
    }
    return ((await res.json()) as { text: string }).text;
  },
};
