/**
 * Alert tones and speech in the webview (live telemetry, experimental). Tones are short beeps
 * made with WebAudio from the patterns in telemetryAlerts.ts: no audio file is shipped and
 * nothing comes from the game. Speech uses the Web Speech API with this computer's own voices
 * only (`localService`): a voice that would send the text to an online service is never used,
 * and with no local voice nothing is spoken (docs/privacy.md).
 *
 * Browsers start audio only after a user gesture: `unlock()` runs on the click that picks
 * "In app + sound" (and on Test), and later tones play once the context is running.
 */
import type { TonePattern } from './telemetryAlerts';

let ctx: AudioContext | null = null;
let current: { stop: () => void } | null = null;

function audio(): AudioContext | null {
  if (ctx) return ctx;
  const AC = (globalThis as { AudioContext?: typeof AudioContext }).AudioContext;
  if (!AC) return null;
  try {
    ctx = new AC();
  } catch {
    ctx = null;
  }
  return ctx;
}

/** Call from a click: lets later alerts make a sound. */
export function unlock(): void {
  const c = audio();
  if (c && c.state === 'suspended') void c.resume().catch(() => {});
}

/** Whether a tone can play right now (the context exists and runs). */
export function canPlay(): boolean {
  return ctx?.state === 'running';
}

/** Stop what is playing (a tone and any speech). */
export function stopAll(): void {
  current?.stop();
  current = null;
  try {
    globalThis.speechSynthesis?.cancel();
  } catch {
    /* no speech here */
  }
}

/** Play a pattern; returns how long it lasts (ms), 0 when nothing could play. */
export function playTone(p: TonePattern, volume = 0.18): number {
  const c = audio();
  if (!c || c.state !== 'running') return 0;
  const t0 = c.currentTime + 0.02;
  const nodes: OscillatorNode[] = [];
  const gain = c.createGain();
  gain.gain.value = 0;
  gain.connect(c.destination);
  for (let i = 0; i < p.beeps; i++) {
    const start = t0 + (i * (p.beepMs + p.gapMs)) / 1000;
    const end = start + p.beepMs / 1000;
    const o = c.createOscillator();
    o.type = p.wave;
    o.frequency.value = p.freq;
    o.connect(gain);
    // Short ramps: no click at either end.
    gain.gain.setValueAtTime(0, start);
    gain.gain.linearRampToValueAtTime(volume, start + 0.01);
    gain.gain.setValueAtTime(volume, end - 0.02);
    gain.gain.linearRampToValueAtTime(0, end);
    o.start(start);
    o.stop(end);
    nodes.push(o);
  }
  const ms = p.beeps * p.beepMs + (p.beeps - 1) * p.gapMs + 20;
  current = {
    stop: () => {
      for (const o of nodes) {
        try {
          o.stop();
        } catch {
          /* already stopped */
        }
      }
      gain.disconnect();
    },
  };
  return ms;
}

/** A voice of this computer (never an online one), or null. */
function localVoice(): SpeechSynthesisVoice | null {
  try {
    const voices = globalThis.speechSynthesis?.getVoices() ?? [];
    const local = voices.filter((v) => v.localService);
    const lang = navigator.language?.slice(0, 2);
    return local.find((v) => v.lang?.startsWith(lang)) ?? local.find((v) => v.lang?.startsWith('en')) ?? local[0] ?? null;
  } catch {
    return null;
  }
}

/** Whether speech can work here (a local voice exists). */
export function canSpeak(): boolean {
  return localVoice() !== null;
}

/** Speak `text` with a local voice after `delayMs`; false when there is none. */
export function speak(text: string, delayMs = 0): boolean {
  const voice = localVoice();
  const synth = globalThis.speechSynthesis;
  if (!voice || !synth) return false;
  const u = new SpeechSynthesisUtterance(text);
  u.voice = voice;
  u.rate = 1.05;
  setTimeout(() => synth.speak(u), delayMs);
  return true;
}
