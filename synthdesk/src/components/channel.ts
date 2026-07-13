import { COL } from '../core/palette'
import { text } from '../render/prims'
import type { ComponentSpec } from './spec'

// channel: balance entre esquerdo e direito. centro passa reto; girar
// pra um lado cala o outro (o caminho da engine e estereo de verdade).
export const channel: ComponentSpec = {
  type: 'channel',
  title: 'CHANNEL',
  tag: 'PAN L/R',
  prefix: 'CHN',
  category: 'CONTROLLERS',
  unitsW: 3,
  unitsH: 4,
  inputs: [{ id: 'in', kind: 'audio', label: 'IN' }],
  outputs: [{ id: 'out', kind: 'audio', label: 'OUT' }],
  params: [{ id: 'pan', label: 'PAN', def: 0.5 }],
  controls: [
    { kind: 'knob', param: 'pan', x: 69, y: 88, r: 30 },
    { kind: 'label', text: 'L', x: 33, y: 116 },
    { kind: 'label', text: 'R', x: 105, y: 116 },
  ],

  cvOut(node, _port, graph, seen) {
    // preview mono: passa o sinal adiante (o estereo e da engine)
    return graph.cvInto(node.id, 'in', seen) ?? node.params.pan ?? 0.5
  },

  drawExtra(g, node) {
    const pan = node.params.pan ?? 0.5
    const pct = Math.round(Math.abs(pan - 0.5) * 200)
    const label = pct === 0 ? 'C' : pan < 0.5 ? `L ${pct}` : `R ${pct}`
    text(g, label, 69, 126, 13, COL.textBright, 'center')
  },
}
