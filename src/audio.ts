export type AudioCue = "move" | "capture" | "win";

class JungleAudio {
  private context: AudioContext | null = null;
  private muted = false;

  setMuted(muted: boolean): void {
    this.muted = muted;
  }

  unlock(): void {
    if (this.muted) return;
    try {
      this.context ??= new AudioContext();
      if (this.context.state === "suspended") void this.context.resume();
    } catch {
      this.context = null;
    }
  }

  play(cue: AudioCue): void {
    if (this.muted) return;
    this.unlock();
    const context = this.context;
    if (!context) return;

    const sequences: Record<AudioCue, Array<[number, number, number]>> = {
      move: [[330, 0, 0.07], [440, 0.055, 0.08]],
      capture: [[190, 0, 0.08], [120, 0.065, 0.12]],
      win: [[392, 0, 0.12], [523, 0.11, 0.14], [659, 0.24, 0.22]],
    };
    const started = context.currentTime;
    for (const [frequency, offset, duration] of sequences[cue]) {
      const oscillator = context.createOscillator();
      const gain = context.createGain();
      oscillator.type = cue === "capture" ? "square" : "sine";
      oscillator.frequency.value = frequency;
      gain.gain.setValueAtTime(0.0001, started + offset);
      gain.gain.exponentialRampToValueAtTime(0.08, started + offset + 0.01);
      gain.gain.exponentialRampToValueAtTime(0.0001, started + offset + duration);
      oscillator.connect(gain).connect(context.destination);
      oscillator.start(started + offset);
      oscillator.stop(started + offset + duration + 0.01);
    }
  }
}

export const jungleAudio = new JungleAudio();

