import type { ComponentSpec } from './spec'
import { math } from './math'
import { mix } from './mix'
import { noise } from './noise'
import { oscillator } from './oscillator'
import { reverb } from './reverb'
import { speaker } from './speaker'
import { volume } from './volume'

// ordem aqui = ordem no components box; itens da mesma categoria
// ficam juntos (o box agrupa por category)
export const COMPONENTS: ComponentSpec[] = [
  speaker, // primitives
  volume, // controllers
  math, // operators
  mix,
  oscillator, // generators
  noise,
  reverb, // effects
]

const byType = new Map(COMPONENTS.map((m) => [m.type, m]))

export function spec(type: string): ComponentSpec {
  const s = byType.get(type)
  if (!s) throw new Error(`componente desconhecido: ${type}`)
  return s
}
