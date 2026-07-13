import { seqSteps } from '../audio/audio'
import { cvToMidi, noteName } from '../core/notes'
import { COL } from '../core/palette'
import type { ComponentSpec } from './spec'

// sequencer: relogio de 8 passos que toca um device. cada celula e um
// passo (ligado dispara); NOTE e GATE saem por cabo pros ports do
// device. PITCH = nota dos passos, RATE = passos por segundo. o
// playhead vem da ENGINE (port.postMessage), entao o que pisca e
// exatamente o que soa.

const CELLS = { x: 16, y: 64, step: 19 }

export const sequencer: ComponentSpec = {
  type: 'sequencer',
  title: 'SEQUENCER',
  tag: 'STEP SEQ',
  prefix: 'SEQ',
  category: 'CONTROLLERS',
  unitsW: 4,
  unitsH: 5,
  inputs: [],
  outputs: [
    { id: 'gate', kind: 'cv', label: 'GATE' },
    { id: 'note', kind: 'cv', label: 'NOTE' },
  ],
  params: [
    { id: 'step1', label: 'S1', def: 1 },
    { id: 'step2', label: 'S2', def: 0 },
    { id: 'step3', label: 'S3', def: 0 },
    { id: 'step4', label: 'S4', def: 0 },
    { id: 'step5', label: 'S5', def: 1 },
    { id: 'step6', label: 'S6', def: 0 },
    { id: 'step7', label: 'S7', def: 0 },
    { id: 'step8', label: 'S8', def: 0 },
    { id: 'pitch', label: 'PITCH', def: 0.5 },
    { id: 'rate', label: 'RATE', def: 0.5 },
  ],
  controls: [
    // celulas dos passos: toggles sem label = zona compacta
    ...Array.from({ length: 8 }, (_, i) => ({
      kind: 'toggle' as const,
      param: `step${i + 1}`,
      x: CELLS.x + i * CELLS.step,
      y: CELLS.y,
      label: '' as const,
    })),
    { kind: 'rule', y: 88 },
    { kind: 'slider', param: 'pitch', x: 14, y: 96, w: 156, label: 'PITCH' },
    { kind: 'slider', param: 'rate', x: 14, y: 124, w: 156, label: 'RATE' },
  ],

  sliderValue(node, param) {
    if (param === 'pitch') return noteName(cvToMidi(node.params.pitch ?? 0.5))
    if (param === 'rate') return `${Math.pow(2, (node.params.rate ?? 0.5) * 4).toFixed(1)}/S`
    return (node.params[param] ?? 0).toFixed(3)
  },

  animates(node) {
    return (node.params.on ?? 1) > 0 // playhead rolando
  },

  // preview de cv: gate do passo atual / pitch
  cvOut(node, port) {
    if (port === 'note') return node.params.pitch ?? 0.5
    const step = seqSteps.get(node.id) ?? 0
    return (node.params[`step${step + 1}`] ?? 0) > 0.5 ? 1 : 0
  },

  drawExtra(g, node) {
    const on = (node.params.on ?? 1) > 0
    // playhead exato reportado pela engine; sem engine (nao ligado num
    // speaker), estimativa local com o mesmo relogio de parede
    const hz = Math.pow(2, (node.params.rate ?? 0.5) * 4)
    const step = seqSteps.get(node.id) ?? Math.floor((performance.now() / 1000) * hz) % 8
    for (let i = 0; i < 8; i++) {
      const x = CELLS.x + i * CELLS.step
      if (on && i === step) {
        g.fillStyle = COL.textBright
        g.fillRect(x + 2, CELLS.y - 8, 6, 3)
      } else {
        g.strokeStyle = COL.lineFaint
        g.strokeRect(x + 3.5, CELLS.y - 7.5, 3, 1)
      }
    }
  },
}
