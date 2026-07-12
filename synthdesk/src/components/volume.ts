import type { ComponentSpec } from './spec'

// knob de volume: atenua o sinal que passa por ele (audio de
// verdade na cadeia do speaker, cv no grafo de controle).
// sem nada no IN, emite o proprio valor do knob como cv.
export const volume: ComponentSpec = {
  type: 'volume',
  title: 'VOLUME',
  tag: 'GAIN',
  prefix: 'VOL',
  category: 'CONTROLLERS',
  unitsW: 3,
  unitsH: 4,
  inputs: [{ id: 'in', kind: 'audio', label: 'IN' }],
  outputs: [{ id: 'out', kind: 'audio', label: 'OUT' }],
  params: [{ id: 'value', label: 'VALUE', def: 0.8 }],
  controls: [
    { kind: 'knob', param: 'value', x: 69, y: 88, r: 30 },
    { kind: 'label', text: '0', x: 33, y: 116 },
    { kind: 'label', text: '1', x: 105, y: 116 },
    { kind: 'readout', param: 'value', x: 69, y: 126 },
  ],

  cvOut(node, _port, graph, seen) {
    const v = node.params.value ?? 0
    const inn = graph.cvInto(node.id, 'in', seen)
    return inn === null ? v : inn * v
  },
}
