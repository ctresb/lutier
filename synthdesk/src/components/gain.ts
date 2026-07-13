import { COL } from '../core/palette'
import { text } from '../render/prims'
import type { ComponentSpec } from './spec'

// knob de gain: amplificador de verdade - 0..2x, unitario no meio
// (volume atenua 0..1; gain tambem EMPURRA). sem nada no IN, emite o
// proprio ganho como tensao.
export const gain: ComponentSpec = {
  type: 'gain',
  title: 'GAIN',
  tag: 'AMP',
  prefix: 'GAN',
  category: 'CONTROLLERS',
  unitsW: 3,
  unitsH: 4,
  inputs: [{ id: 'in', kind: 'audio', label: 'IN' }],
  outputs: [{ id: 'out', kind: 'audio', label: 'OUT' }],
  params: [{ id: 'value', label: 'GAIN', def: 0.5 }],
  controls: [
    { kind: 'knob', param: 'value', x: 69, y: 88, r: 30 },
    { kind: 'label', text: '0', x: 33, y: 116 },
    { kind: 'label', text: '2', x: 105, y: 116 },
  ],

  cvOut(node, _port, graph, seen) {
    const k = (node.params.value ?? 0.5) * 2
    const inn = graph.cvInto(node.id, 'in', seen)
    return inn === null ? k : inn * k
  },

  drawExtra(g, node) {
    // leitura em fator de multiplicacao
    const k = (node.params.value ?? 0.5) * 2
    text(g, `X${k.toFixed(2)}`, 69, 126, 13, COL.textBright, 'center')
  },
}
