import { COL, ph } from '../core/palette'
import type { ComponentSpec } from './spec'

// gerador de ruido: white / pink / brown. viewport QUADRADO com o
// grao animado em tempo real seguindo o que sai do componente:
// tipo muda a textura (white = grao fino e rapido, pink = medio,
// brown = blocos lentos), LEVEL clareia/apaga o grao e DENSITY
// esparsa as celulas (na engine: chance por amostra de renovar o
// sample - baixo = lo-fi granulado, quase estalos).

export const NOISE_TYPES = ['white', 'pink', 'brown'] as const
const TYPE_LABELS = ['WHITE', 'PINK', 'BROWN']

// quadrado do viewport (4u de largura -> centrado)
const SQ = { x: 48, y: 50, s: 88 }

// grao por tipo: [tamanho da celula, periodo do frame em ms, alpha]
const GRAIN: [number, number, number][] = [
  [2, 16, 0.42],
  [3, 45, 0.46],
  [6, 110, 0.52],
]

function xorshift32(s: number): number {
  s ^= s << 13
  s >>>= 0
  s ^= s >>> 17
  s ^= s << 5
  return s >>> 0
}

export const noise: ComponentSpec = {
  type: 'noise',
  title: 'NOISE',
  tag: 'NOISE GEN',
  prefix: 'NSE',
  category: 'GENERATORS',
  unitsW: 4,
  unitsH: 6,
  inputs: [],
  outputs: [{ id: 'out', kind: 'audio', label: 'OUT' }],
  params: [
    { id: 'type', label: 'TYPE', def: 0 },
    { id: 'level', label: 'LEVEL', def: 0.8 },
    { id: 'density', label: 'DENSITY', def: 1 },
  ],
  controls: [
    { kind: 'rule', y: 146 },
    { kind: 'selector', id: 'type', x: 14, y: 154, label: 'TYPE' },
    { kind: 'rule', y: 170 },
    { kind: 'slider', param: 'level', x: 14, y: 178, w: 156, label: 'LEVEL' },
    { kind: 'slider', param: 'density', x: 14, y: 206, w: 156, label: 'DENSITY' },
  ],

  press(node, button) {
    if (button === 'type') {
      node.params.type = ((node.params.type ?? 0) + 1) % NOISE_TYPES.length
    }
  },

  selectorValue(node) {
    return TYPE_LABELS[node.params.type ?? 0] ?? 'WHITE'
  },

  animates(node) {
    // ligado = grao mexendo em tempo real
    return (node.params.on ?? 1) > 0
  },

  // preview de cv: amostra pseudo-aleatoria estavel por frame (MATH/
  // MIX mostram o sinal chacoalhando), normalizada 0..1
  cvOut(node) {
    let s = (Math.floor(performance.now() / 16) * 2654435761) ^ (node.id * 40503)
    s = xorshift32(s >>> 0 || 1)
    const w = s / 4294967296 - 0.5
    return 0.5 + w * (node.params.level ?? 0.8)
  },

  drawExtra(g, node) {
    const on = (node.params.on ?? 1) > 0
    const type = node.params.type ?? 0
    const level = node.params.level ?? 0.8
    const density = node.params.density ?? 1
    const [cs, period, alpha] = GRAIN[type] ?? GRAIN[0]

    // moldura do viewport, mesma linguagem do scope
    g.strokeStyle = COL.lineFaint
    g.strokeRect(SQ.x, SQ.y, SQ.s, SQ.s)

    // desligado congela o grao (seed fixa) e esmaece (regra da base)
    let seed = on
      ? ((Math.floor(performance.now() / period) * 2654435761) ^ (node.id * 40503)) >>> 0
      : (node.id * 40503) >>> 0
    if (seed === 0) seed = 1
    if (!on) g.globalAlpha = 0.4

    // level clareia o grao, density esparsa as celulas acesas -
    // o viewport mostra exatamente o que os sliders fazem no som
    const gain = 0.15 + 0.85 * level
    const thr = 0.55 + (1 - density) * 0.4
    const styles = [
      ph(255, alpha * gain * 0.25),
      ph(255, alpha * gain * 0.5),
      ph(255, alpha * gain * 0.75),
      ph(255, alpha * gain),
    ]
    const n = Math.floor(SQ.s / cs)
    const pad = (SQ.s - n * cs) / 2
    for (let iy = 0; iy < n; iy++) {
      for (let ix = 0; ix < n; ix++) {
        seed = xorshift32(seed)
        const r = seed / 4294967296
        if (r < thr) continue // celula apagada: o preto e parte do grao
        g.fillStyle = styles[Math.min(3, Math.floor(((r - thr) / (1 - thr)) * 4))]
        g.fillRect(SQ.x + pad + ix * cs, SQ.y + pad + iy * cs, cs - 1, cs - 1)
      }
    }
    g.globalAlpha = 1
  },
}
