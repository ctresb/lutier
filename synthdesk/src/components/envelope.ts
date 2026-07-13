import { COL } from '../core/palette'
import type { ComponentSpec } from './spec'

// envelope: PROPRIEDADE de device (fadein/decay/sustain/fadeout).
// plugado no port ENV de um device, dita o ADSR das notas; o viewport
// mostra a curva exata. como sinal, emite o sustain (e uma descricao,
// nao um gerador).

const VP = { x: 12, y: 50, w: 160, h: 48 }

// mesmos mapeamentos da engine (processor.ts)
export const envSeconds = {
  attack: (v: number) => 0.002 + v * v * 2,
  decay: (v: number) => 0.005 + v * v * 2,
  release: (v: number) => 0.005 + v * v * 3,
}

function fmtSeconds(s: number): string {
  return s < 1 ? `${Math.round(s * 1000)}MS` : `${s.toFixed(2)}S`
}

export const envelope: ComponentSpec = {
  type: 'envelope',
  title: 'ENVELOPE',
  tag: 'DEV PROP',
  prefix: 'ENV',
  category: 'PROPERTIES',
  unitsW: 4,
  unitsH: 6,
  inputs: [],
  outputs: [{ id: 'out', kind: 'cv', label: 'OUT' }],
  params: [
    { id: 'attack', label: 'ATTACK', def: 0.05 },
    { id: 'decay', label: 'DECAY', def: 0.3 },
    { id: 'sustain', label: 'SUSTAIN', def: 0.7 },
    { id: 'release', label: 'RELEASE', def: 0.25 },
  ],
  controls: [
    { kind: 'rule', y: 106 },
    { kind: 'slider', param: 'attack', x: 14, y: 114, w: 156, label: 'ATTACK' },
    { kind: 'slider', param: 'decay', x: 14, y: 142, w: 156, label: 'DECAY' },
    { kind: 'slider', param: 'sustain', x: 14, y: 170, w: 156, label: 'SUSTAIN' },
    { kind: 'slider', param: 'release', x: 14, y: 198, w: 156, label: 'RELEASE' },
  ],

  sliderValue(node, param) {
    const v = node.params[param] ?? 0
    if (param === 'sustain') return v.toFixed(3)
    return fmtSeconds(envSeconds[param as keyof typeof envSeconds](v))
  },

  cvOut(node) {
    return node.params.sustain ?? 0.7
  },

  drawExtra(g, node) {
    const on = (node.params.on ?? 1) > 0
    const a = node.params.attack ?? 0.05
    const d = node.params.decay ?? 0.3
    const s = node.params.sustain ?? 0.7
    const r = node.params.release ?? 0.25

    g.strokeStyle = COL.lineFaint
    g.strokeRect(VP.x, VP.y, VP.w, VP.h)
    if (!on) g.globalAlpha = 0.4

    // segmentos proporcionais aos tempos, sustain com fatia fixa
    const pad = 6
    const x0 = VP.x + pad
    const usable = VP.w - pad * 2
    const susW = usable * 0.22
    const tA = envSeconds.attack(a)
    const tD = envSeconds.decay(d)
    const tR = envSeconds.release(r)
    const tot = tA + tD + tR
    const wA = ((usable - susW) * tA) / tot
    const wD = ((usable - susW) * tD) / tot
    const wR = ((usable - susW) * tR) / tot
    const yBot = VP.y + VP.h - 8
    const yTop = VP.y + 8
    const ySus = yBot - (yBot - yTop) * s

    g.strokeStyle = COL.line
    g.beginPath()
    g.moveTo(x0, yBot)
    g.lineTo(x0 + wA, yTop)
    g.lineTo(x0 + wA + wD, ySus)
    g.lineTo(x0 + wA + wD + susW, ySus)
    g.lineTo(x0 + wA + wD + susW + wR, yBot)
    g.stroke()

    // juntas marcadas (a-d-s-r)
    g.fillStyle = COL.textBright
    for (const [jx, jy] of [
      [x0 + wA, yTop],
      [x0 + wA + wD, ySus],
      [x0 + wA + wD + susW, ySus],
    ]) {
      g.fillRect(jx - 1.5, jy - 1.5, 3, 3)
    }
    g.globalAlpha = 1
  },
}
