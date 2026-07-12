import { COL } from '../core/palette'
import { text } from '../render/prims'
import type { ComponentSpec } from './spec'

// operador de cv: dois sinais entram, um resultado sai.
// entrada desconectada vale 0.
const OPS = ['ADD', 'SUB', 'MUL', 'DIV', 'MIN', 'MAX', 'AVG'] as const

export function mathApply(op: number, a: number, b: number): number {
  switch (OPS[op] ?? 'ADD') {
    case 'SUB':
      return a - b
    case 'MUL':
      return a * b
    case 'DIV':
      return b === 0 ? 0 : a / b
    case 'MIN':
      return Math.min(a, b)
    case 'MAX':
      return Math.max(a, b)
    case 'AVG':
      return (a + b) / 2
    default:
      return a + b
  }
}

export const math: ComponentSpec = {
  type: 'math',
  title: 'MATH',
  tag: 'CV OP',
  prefix: 'MTH',
  category: 'OPERATORS',
  unitsW: 3,
  unitsH: 4,
  inputs: [
    { id: 'a', kind: 'cv', label: 'A' },
    { id: 'b', kind: 'cv', label: 'B' },
  ],
  outputs: [{ id: 'out', kind: 'cv', label: 'OUT' }],
  params: [{ id: 'op', label: 'OP', def: 0 }],
  controls: [
    { kind: 'selector', id: 'op', x: 14, y: 48, label: 'OP' },
    { kind: 'rule', y: 63 },
    { kind: 'rule', y: 108 },
  ],

  press(node, button) {
    if (button === 'op') node.params.op = ((node.params.op ?? 0) + 1) % OPS.length
  },

  selectorValue(node) {
    return OPS[node.params.op ?? 0] ?? 'ADD'
  },

  cvOut(node, _port, graph, seen) {
    const a = graph.cvInto(node.id, 'a', seen) ?? 0
    const b = graph.cvInto(node.id, 'b', seen) ?? 0
    return mathApply(node.params.op ?? 0, a, b)
  },

  drawExtra(g, node, o) {
    // entradas ao vivo + resultado grande
    const a = o.cvInto('a') ?? 0
    const b = o.cvInto('b') ?? 0
    const out = mathApply(node.params.op ?? 0, a, b)
    text(g, 'A', 14, 72, 9, COL.textFaint)
    text(g, a.toFixed(3), 124, 72, 9, COL.textDim, 'right')
    text(g, 'B', 14, 88, 9, COL.textFaint)
    text(g, b.toFixed(3), 124, 88, 9, COL.textDim, 'right')
    text(g, out.toFixed(3), 69, 120, 16, COL.textBright, 'center')
  },
}
