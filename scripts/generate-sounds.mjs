import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const directory = fileURLToPath(new URL('../public/sounds/', import.meta.url));
mkdirSync(directory, { recursive: true });
const rate = 44100;
function wav(name, duration, sample) {
  const count = Math.floor(rate * duration);
  const bytes = Buffer.alloc(44 + count * 2);
  bytes.write('RIFF', 0); bytes.writeUInt32LE(36 + count * 2, 4); bytes.write('WAVEfmt ', 8);
  bytes.writeUInt32LE(16, 16); bytes.writeUInt16LE(1, 20); bytes.writeUInt16LE(1, 22);
  bytes.writeUInt32LE(rate, 24); bytes.writeUInt32LE(rate * 2, 28);
  bytes.writeUInt16LE(2, 32); bytes.writeUInt16LE(16, 34); bytes.write('data', 36);
  bytes.writeUInt32LE(count * 2, 40);
  for (let index = 0; index < count; index++) {
    const t = index / rate;
    const envelope = Math.min(t / .004, 1) * Math.exp(-t * 18) * Math.min((duration - t) / .025, 1);
    bytes.writeInt16LE(Math.round(Math.max(-1, Math.min(1, sample(t) * envelope)) * 32767), 44 + index * 2);
  }
  writeFileSync(`${directory}/${name}.wav`, bytes);
}
const tone = (frequency, t) => Math.sin(2 * Math.PI * frequency * t);
// Original synthesized cues. No third-party samples or Persona recordings.
wav('complete', .29, t => .13 * tone(784, t) + .095 * tone(1174.66, t) + .035 * tone(1568, t));
wav('undo', .16, t => .12 * tone(523.25, t) + .055 * tone(392, t));
console.log('Generated original completion and undo cues.');
