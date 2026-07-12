import { COL, ph } from '../core/palette'
import { text } from '../render/prims'
import type { ComponentSpec } from './spec'

// reverb (estilo fl studio): sala visualizada como um prisma 3D de N
// paredes (mais paredes = mais cilindro), girando na velocidade do
// SPEED. WALLS (slider com detents 3..8) = quantas linhas de atraso
// na engine; TYPE muda o carater (tamanho/abafamento) e as proporcoes
// da sala; DRY acende a FONTE no centro da sala, WET acende as
// PAREDES (o bloom do cilindro cresce com o molhado); SPEED = quao
// rapido a cauda morre.

export const REVERB_TYPES = ['room', 'hall', 'plate', 'cave'] as const
const TYPE_LABELS = ['ROOM', 'HALL', 'PLATE', 'CAVE']
// proporcoes da sala por tipo: [altura, raio]
const SHAPE: [number, number][] = [
  [34, 34],
  [54, 28],
  [22, 42],
  [62, 36],
]

// walls guarda 0..1 com 6 detents; a engine e o desenho leem 3..8
export function wallCount(v: number): number {
  return 3 + Math.round(Math.min(1, Math.max(0, v)) * 5)
}

// viewport da sala (coluna esquerda do miolo)
const VP = { x: 12, y: 50, w: 108, h: 88 }

// rotacao acumulada por node: velocidade muda sem pular a fase,
// desligar congela (mesma logica do scope do oscillator)
const spinState = new Map<number, { rot: number; t: number }>()

function spin(id: number, on: boolean, rate: number): number {
  const now = performance.now() / 1000
  let s = spinState.get(id)
  if (!s) {
    s = { rot: 0, t: now }
    spinState.set(id, s)
  }
  if (on) s.rot += (now - s.t) * rate
  s.t = now
  return s.rot
}

export const reverb: ComponentSpec = {
  type: 'reverb',
  title: 'REVERB',
  tag: 'FX VERB',
  prefix: 'RVB',
  category: 'EFFECTS',
  unitsW: 4,
  unitsH: 7,
  inputs: [{ id: 'in', kind: 'audio', label: 'IN' }],
  outputs: [{ id: 'out', kind: 'audio', label: 'OUT' }],
  params: [
    { id: 'walls', label: 'WALLS', def: 0.4 }, // detent 3 de 6 -> 5 paredes
    { id: 'type', label: 'TYPE', def: 0 },
    { id: 'dry', label: 'DRY', def: 0.8 },
    { id: 'wet', label: 'WET', def: 0.35 },
    { id: 'speed', label: 'SPEED', def: 0.5 },
  ],
  controls: [
    { kind: 'label', text: 'SPEED', x: 150, y: 50 },
    { kind: 'knob', param: 'speed', x: 150, y: 90, r: 16 },
    { kind: 'readout', param: 'speed', x: 150, y: 114 },
    { kind: 'rule', y: 146 },
    { kind: 'selector', id: 'type', x: 14, y: 154, label: 'TYPE' },
    { kind: 'rule', y: 170 },
    { kind: 'slider', param: 'walls', x: 14, y: 178, w: 156, label: 'WALLS', steps: 6 },
    { kind: 'slider', param: 'dry', x: 14, y: 206, w: 156, label: 'DRY' },
    { kind: 'slider', param: 'wet', x: 14, y: 234, w: 156, label: 'WET' },
  ],

  press(node, button) {
    if (button === 'type') {
      node.params.type = ((node.params.type ?? 0) + 1) % REVERB_TYPES.length
    }
  },

  selectorValue(node) {
    return TYPE_LABELS[node.params.type ?? 0] ?? 'ROOM'
  },

  sliderValue(node, param) {
    if (param === 'walls') return String(wallCount(node.params.walls ?? 0.4)).padStart(2, '0')
    return (node.params[param] ?? 0).toFixed(3)
  },

  animates(node) {
    // ligado = sala girando (a rotacao E a leitura do speed)
    return (node.params.on ?? 1) > 0
  },

  // efeito passa o sinal adiante no grafo de cv (o molhado e da engine)
  cvOut(node, _port, graph, seen) {
    return graph.cvInto(node.id, 'in', seen) ?? 0
  },

  drawExtra(g, node) {
    const on = (node.params.on ?? 1) > 0
    const type = node.params.type ?? 0
    const walls = wallCount(node.params.walls ?? 0.4)
    const speed = node.params.speed ?? 0.5
    const dry = node.params.dry ?? 0.8
    const wet = node.params.wet ?? 0.35
    const [hgt, rx] = SHAPE[type] ?? SHAPE[0]
    const ry = Math.round(rx * 0.32)

    // moldura do viewport, mesma linguagem do scope
    g.strokeStyle = COL.lineFaint
    g.strokeRect(VP.x, VP.y, VP.w, VP.h)

    const cx = VP.x + VP.w / 2
    const cy = VP.y + VP.h / 2
    const yT = cy - hgt / 2
    const yB = cy + hgt / 2
    const rot = spin(node.id, on, 0.25 + speed * 1.25)
    if (!on) g.globalAlpha = 0.4

    // wet acende as paredes: quanto mais molhado, mais a sala brilha
    // (e o bloom global responde ao brilho, sem glow desenhado a mao)
    const frontCol = ph(70 + 165 * wet)
    const rimCol = ph(90 + 145 * wet)
    const backCol = ph(30 + 60 * wet)

    // vertices do prisma de N paredes projetado (topo/base elipticos)
    const vx: number[] = []
    const vs: number[] = [] // sin(a): >0 = frente
    for (let i = 0; i < walls; i++) {
      const a = rot + (i * 2 * Math.PI) / walls
      vx.push(cx + Math.cos(a) * rx)
      vs.push(Math.sin(a))
    }
    const edge = (x0: number, y0: number, x1: number, y1: number, front: boolean, rim: boolean): void => {
      g.strokeStyle = front ? (rim ? rimCol : frontCol) : backCol
      g.beginPath()
      g.moveTo(x0, y0)
      g.lineTo(x1, y1)
      g.stroke()
    }
    for (let i = 0; i < walls; i++) {
      const j = (i + 1) % walls
      const yiT = yT + vs[i] * ry
      const yiB = yB + vs[i] * ry
      const yjT = yT + vs[j] * ry
      const yjB = yB + vs[j] * ry
      // parede (aresta vertical) + aros do topo e da base; frente
      // brilha mais que o fundo (profundidade por brilho, nunca cor)
      edge(vx[i], yiT, vx[i], yiB, vs[i] > 0, false)
      const front = vs[i] + vs[j] > 0
      edge(vx[i], yiT, vx[j], yjT, front, true)
      edge(vx[i], yiB, vx[j], yjB, front, false)
    }

    // dry acende a FONTE no centro da sala: quadradinho preenchido
    // (linguagem do port ativo), brilho = quanto de seco passa
    g.fillStyle = ph(60 + 187 * dry)
    g.fillRect(cx - 2, cy - 2, 4, 4)
    g.globalAlpha = 1

    // leitura do tamanho da sala no canto do viewport
    text(g, `${walls}W`, VP.x + VP.w - 5, VP.y + VP.h - 13, 8, COL.textFaint, 'right')
  },
}
