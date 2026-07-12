import { COL } from '../core/palette'
import { text } from '../render/prims'
import type { ComponentSpec } from './spec'

// mixer de dois canais: cada knob dita quantos % da entrada vao pro
// out (out = a * ka + b * kb). entrada desconectada vale 0.
export const mix: ComponentSpec = {
  type: 'mix',
  title: 'MIX',
  tag: 'CV MIX',
  prefix: 'MIX',
  category: 'OPERATORS',
  unitsW: 3,
  unitsH: 4,
  inputs: [
    { id: 'a', kind: 'cv', label: 'A' },
    { id: 'b', kind: 'cv', label: 'B' },
  ],
  outputs: [{ id: 'out', kind: 'cv', label: 'OUT' }],
  params: [
    { id: 'ka', label: 'A %', def: 0.5 },
    { id: 'kb', label: 'B %', def: 0.5 },
  ],
  controls: [
    { kind: 'label', text: 'A', x: 40, y: 50 },
    { kind: 'label', text: 'B', x: 98, y: 50 },
    { kind: 'knob', param: 'ka', x: 40, y: 82, r: 16 },
    { kind: 'knob', param: 'kb', x: 98, y: 82, r: 16 },
    { kind: 'rule', y: 118 },
  ],

  cvOut(node, _port, graph, seen) {
    const a = graph.cvInto(node.id, 'a', seen) ?? 0
    const b = graph.cvInto(node.id, 'b', seen) ?? 0
    return a * (node.params.ka ?? 0) + b * (node.params.kb ?? 0)
  },

  drawExtra(g, node, o) {
    // porcentagens sob os knobs + resultado ao vivo
    const ka = node.params.ka ?? 0
    const kb = node.params.kb ?? 0
    text(g, `${Math.round(ka * 100)}%`, 40, 106, 9, COL.text, 'center')
    text(g, `${Math.round(kb * 100)}%`, 98, 106, 9, COL.text, 'center')
    const a = o.cvInto('a') ?? 0
    const b = o.cvInto('b') ?? 0
    text(g, (a * ka + b * kb).toFixed(3), 69, 128, 13, COL.textBright, 'center')
  },
}
