type Cue = 'complete' | 'undo';
let context: AudioContext | undefined;
const encoded = new Map<Cue, Promise<ArrayBuffer>>();
const decoded = new Map<Cue, Promise<AudioBuffer>>();
const playing = new Set<AudioBufferSourceNode>();
let muted = false;

export function preloadSounds() {
  for (const cue of ['complete', 'undo'] as const) {
    if (!encoded.has(cue)) {
      const bytes = fetch(`/sounds/${cue}.wav`).then(response => {
        if (!response.ok) throw new Error('Audio unavailable');
        return response.arrayBuffer();
      });
      // A missing audio device or asset never prevents task management.
      void bytes.catch(() => {});
      encoded.set(cue, bytes);
    }
  }
}

export function primeAudio() {
  if (muted) return;
  try {
    context ??= new AudioContext();
    void context.resume().catch(() => {});
    preloadSounds();
    for (const [cue, bytes] of encoded) {
      if (!decoded.has(cue)) {
        const buffer = bytes.then(data => context!.decodeAudioData(data.slice(0)));
        void buffer.catch(() => {});
        decoded.set(cue, buffer);
      }
    }
  } catch { /* Audio is optional; persisted task feedback still has text and visuals. */ }
}

export function muteAudio(value: boolean) {
  muted = value;
  if (value) {
    for (const source of playing) { try { source.stop(); } catch { /* already stopped */ } }
    playing.clear();
  }
}

export async function playCue(cue: Cue) {
  if (muted) return;
  primeAudio();
  try {
    const buffer = await decoded.get(cue);
    if (!buffer || !context || muted) return;
    if (playing.size >= 5) playing.values().next().value?.stop();
    const source = context.createBufferSource();
    source.buffer = buffer;
    source.connect(context.destination);
    playing.add(source);
    source.onended = () => { playing.delete(source); source.disconnect(); };
    source.start();
  } catch { /* Sound failure must never become a task-save failure. */ }
}
