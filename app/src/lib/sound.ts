// The sound that goes with a notification, made on the spot so Hub ships no audio files.

let ctx: AudioContext | null = null;

/** Plays a short sound: "soft" is one low note, "bell" two higher ones. "off" plays nothing. */
export function chime(kind: "off" | "soft" | "bell") {
  if (kind === "off") return;
  try {
    ctx ??= new AudioContext();
    const notes = kind === "soft" ? [523.25] : [880, 1318.5];
    notes.forEach((freq, i) => {
      const at = ctx!.currentTime + i * 0.12;
      const osc = ctx!.createOscillator();
      const gain = ctx!.createGain();
      osc.type = "sine";
      osc.frequency.value = freq;
      gain.gain.setValueAtTime(0, at);
      gain.gain.linearRampToValueAtTime(0.18, at + 0.01);
      gain.gain.exponentialRampToValueAtTime(0.001, at + 0.5);
      osc.connect(gain).connect(ctx!.destination);
      osc.start(at);
      osc.stop(at + 0.55);
    });
  } catch {
    // No audio device; the notification still shows.
  }
}
