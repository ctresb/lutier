import { cvToMidi, noteName } from '../core/notes'
import { COL } from '../core/palette'
import { text } from '../render/prims'
import type { ComponentSpec } from './spec'

// device: o INSTRUMENTO da mesa. o que estiver plugado no IN e o
// timbre (osciladores, noise, efeitos); tocar uma nota transpoe os
// osciladores desse cone e dispara o envelope. a nota vem do teclado
// (device selecionado, teclas Z-M = C4..C5), de um sequencer plugado
// em NOTE/GATE, ou das variaveis globais (DEV_01_NOTE / DEV_01_GATE).
// propriedades sao COMPONENTES: um envelope plugado no ENV da o
// fadein/decay/sustain/fadeout.

export const device: ComponentSpec = {
  type: 'device',
  title: 'DEVICE',
  tag: 'INSTRUMENT',
  prefix: 'DEV',
  category: 'PRIMITIVES',
  unitsW: 5,
  unitsH: 4,
  inputs: [
    { id: 'in', kind: 'audio', label: 'IN' },
    { id: 'note', kind: 'cv', label: 'NOTE' },
    { id: 'gate', kind: 'cv', label: 'GATE' },
    { id: 'env', kind: 'cv', label: 'ENV' },
  ],
  outputs: [{ id: 'out', kind: 'audio', label: 'OUT' }],
  params: [
    { id: 'note', label: 'NOTE', def: 60 },
    { id: 'gate', label: 'GATE', def: 0 },
  ],
  controls: [{ kind: 'rule', y: 106 }],

  animates(node, graph) {
    if ((node.params.on ?? 1) <= 0) return false
    // com gate/note vindo de cabo (sequencer), o painel segue ao vivo
    return (
      (node.params.gate ?? 0) > 0 ||
      (graph?.cables.some((c) => c.to.node === node.id && (c.to.port === 'gate' || c.to.port === 'note')) ??
        false)
    )
  },

  // preview de cv: o timbre passa quando o gate esta aberto
  cvOut(node, _port, graph, seen) {
    const gate = graph.cvInto(node.id, 'gate', seen) ?? (node.params.gate ?? 0)
    return gate > 0.5 ? (graph.cvInto(node.id, 'in', seen) ?? 0) : 0
  },

  drawExtra(g, node, o) {
    const on = (node.params.on ?? 1) > 0
    // cabo ganha do param, igual na engine
    const noteCv = o.cvInto('note')
    const midi = noteCv !== null ? cvToMidi(noteCv) : (node.params.note ?? 60)
    const gate = (o.cvInto('gate') ?? node.params.gate ?? 0) > 0.5

    // nota atual em destaque + estado do gate (linguagem do port)
    text(g, 'NOTE', 14, 52, 9, COL.textFaint)
    text(g, noteName(midi), 14, 66, 14, on && gate ? COL.textBright : COL.textDim)
    text(g, 'GATE', 216, 52, 9, COL.textFaint, 'right')
    const gx = 206
    g.strokeStyle = gate && on ? COL.textBright : COL.lineMid
    g.strokeRect(gx, 66, 10, 10)
    if (gate && on) {
      g.fillStyle = COL.textBright
      g.fillRect(gx + 3, 69, 4, 4)
    }

    // hint de teclado quando selecionado
    if (o.selected) text(g, 'KEYS Z-M', 14, 88, 9, COL.textFaint)
    text(g, 'IN = TIMBRE // ENV = PROPS', 216, 88, 8, COL.textFaint, 'right')
  },
}
