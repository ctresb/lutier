// regua de notas da mesa: cv 0..1 <-> midi 36..84 (C2..C6), a mesma
// em todo lugar (device, sequencer, engine)
const NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B']

export function noteName(midi: number): string {
  const m = Math.round(midi)
  return `${NAMES[((m % 12) + 12) % 12]}${Math.floor(m / 12) - 1}`
}

export function cvToMidi(v: number): number {
  return 36 + Math.min(1, Math.max(0, v)) * 48
}
