import { COL, ph } from '../core/palette'
import { text } from '../render/prims'
import type { ComponentSpec } from './spec'

// gerador de ondas: sine / square / triangle / sawtooth, com scope
// em tempo real e switch de on/off. freq por knob (20..2560hz, curva
// de oitavas) ou por cv no port FREQ (pot conectado muda a altura).

export const WAVES = ['sine', 'square', 'triangle', 'sawtooth'] as const
const WAVE_LABELS = ['SINE', 'SQUARE', 'TRIANGLE', 'SAWTOOTH']

// mapeamento cv 0..1 -> hz: curva de oitavas ancorada no ZERO
// (v=0 -> 0hz, v=1 -> ~2540hz)
export function oscFreq(v: number): number {
  return 20 * (Math.pow(2, Math.min(1, Math.max(0, v)) * 7) - 1)
}

// inversa da curva: hz -> posicao do knob
export function invOscFreq(hz: number): number {
  return Math.min(1, Math.max(0, Math.log2(Math.max(0, hz) / 20 + 1) / 7))
}

// geometria do scope (4u de largura)
const SCOPE = { x: 12, y: 50, w: 160, h: 48 }

// a REDE da mesa: um relogio global unico alimenta todos os
// osciladores (fase = freq * T da rede). dois osciladores em 80hz e
// 40hz ficam travados em fase pra sempre - qualquer razao de
// frequencia alinha automaticamente, como corrente alternada
// alimentando tudo. desligar congela o desenho na ultima fase;
// religar re-engata na rede (como plugar de volta na tomada).
const MAINS_T0 = performance.now()

function mainsTime(): number {
  return (performance.now() - MAINS_T0) / 1000
}

const frozen = new Map<number, number>()

function oscPhase(id: number, on: boolean, freq: number): number {
  if (on) {
    frozen.delete(id)
    return freq * mainsTime()
  }
  let f = frozen.get(id)
  if (f === undefined) {
    f = freq * mainsTime()
    frozen.set(id, f)
  }
  return f
}

function waveSample(wave: number, t: number): number {
  const f = t - Math.floor(t)
  switch (wave) {
    case 1: // square
      return f < 0.5 ? 1 : -1
    case 2: // triangle
      return 4 * Math.abs(f - 0.5) - 1
    case 3: // sawtooth
      return 2 * f - 1
    default: // sine
      return Math.sin(f * Math.PI * 2)
  }
}

export const oscillator: ComponentSpec = {
  type: 'oscillator',
  title: 'OSCILLATOR',
  tag: 'WAVE GEN',
  prefix: 'OSC',
  category: 'GENERATORS',
  unitsW: 4,
  unitsH: 5,
  // cv no FREQ manda na altura (curva de oitavas, mesma do knob); a
  // engine le esse cv em taxa de audio, entao oscilador em oscilador
  // vira fm exponencial
  inputs: [{ id: 'freq', kind: 'cv', label: 'FREQ' }],
  outputs: [{ id: 'out', kind: 'audio', label: 'OUT' }],
  // ligar/desligar e o switch de energia da base, no header
  params: [
    { id: 'wave', label: 'WAVE', def: 0 },
    { id: 'freq', label: 'FREQ', def: 0.64 }, // ~443hz
  ],
  controls: [
    { kind: 'rule', y: 106 },
    { kind: 'selector', id: 'wave', x: 14, y: 114, label: 'WAVE' },
    { kind: 'rule', y: 129 },
    // embaixo: knob de freq a esquerda, leitura em hz a direita
    { kind: 'knob', param: 'freq', x: 48, y: 162, r: 18 },
  ],

  press(node, button) {
    if (button === 'wave') {
      node.params.wave = ((node.params.wave ?? 0) + 1) % WAVES.length
    }
  },

  // knob de freq anda em hz: sem shift so a unidade muda (o decimo
  // fica onde esta), com shift ajusta de 0.1 em 0.1
  knobMap(param) {
    if (param !== 'freq') return null
    return { to: oscFreq, from: invOscFreq, step: (fine) => (fine ? 0.1 : 1) }
  },

  selectorValue(node) {
    return WAVE_LABELS[node.params.wave ?? 0] ?? 'SINE'
  },

  animates(node, graph) {
    // ligado e oscilando = scope rolando em tempo real, sempre
    if ((node.params.on ?? 1) <= 0) return false
    const cv = graph?.cvInto(node.id, 'freq')
    return oscFreq(cv ?? node.params.freq ?? 0) > 0
  },

  // preview da onda em taxa de controle pro grafo de cv (MATH/MIX
  // mostram o sinal oscilando, na MESMA fase do scope); normalizado
  // 0..1 como todo cv da mesa
  cvOut(node, _port, graph, seen) {
    const cv = graph.cvInto(node.id, 'freq', seen)
    const freq = oscFreq(cv ?? node.params.freq ?? 0)
    if (freq <= 0) return 0.5 // 0hz = sinal parado no centro
    // valor instantaneo REAL da onda na fase acumulada
    const on = (node.params.on ?? 1) > 0
    return (waveSample(node.params.wave ?? 0, oscPhase(node.id, on, freq)) + 1) / 2
  },

  drawExtra(g, node, o) {
    const on = (node.params.on ?? 1) > 0
    const wave = node.params.wave ?? 0
    // cv plugado no FREQ manda; senao vale o knob
    const freq = oscFreq(o.cvInto('freq') ?? node.params.freq ?? 0)

    // scope de tempo real SEM regimes: a janela encolhe SUAVE com a
    // frequencia (1/sqrt(hz)), entao os ciclos visiveis crescem
    // continuamente com o knob (ciclos = sqrt(hz): 1hz = 1 ciclo em
    // 1s, 100hz = 10 ciclos, 2500hz = 50). nada salta, nada trava.
    const win = freq > 1 ? 1 / Math.sqrt(freq) : 1

    // moldura + linha de centro
    g.strokeStyle = COL.lineFaint
    g.strokeRect(SCOPE.x, SCOPE.y, SCOPE.w, SCOPE.h)
    const my = SCOPE.y + SCOPE.h / 2
    g.strokeStyle = ph(40)
    g.beginPath()
    g.moveTo(SCOPE.x + 4, my)
    g.lineTo(SCOPE.x + SCOPE.w - 4, my)
    g.stroke()

    // barrinhas de tempo no topo (como os ticks do knob): fracoes
    // da janela em 10 divisoes, maior nas metades
    const x0 = SCOPE.x + 6
    const iw = SCOPE.w - 12
    g.strokeStyle = COL.lineFaint
    g.beginPath()
    for (let i = 0; i <= 10; i++) {
      const tx = x0 + (i / 10) * iw
      g.moveTo(tx, SCOPE.y + 1)
      g.lineTo(tx, SCOPE.y + (i % 5 === 0 ? 7 : 4))
    }
    g.stroke()
    // quanto tempo cabe na janela (continuo, informativo)
    const ms = win * 1000
    const winLabel = win >= 1 ? '1S' : `${ms >= 100 ? Math.round(ms) : ms.toFixed(1)}MS`
    text(g, winLabel, SCOPE.x + SCOPE.w - 5, SCOPE.y + 10, 8, COL.textFaint, 'right')

    // rola pela fase ACUMULADA (congela no off, retoma exato);
    // traco HAIRLINE (1px de tela, igual aos ticks do knob)
    const base = oscPhase(node.id, on && freq > 0, freq)
    const amp = freq > 0 ? SCOPE.h / 2 - 9 : 0
    const hair = 1 / o.zoom
    g.strokeStyle = on ? COL.textBright : COL.textFaint
    g.lineWidth = hair
    const pxPerCycle = freq > 0 ? iw / (freq * win) : Infinity

    if (pxPerCycle >= 5) {
      // ciclos resolviveis: linha pura, 2 amostras por pixel
      // (picos suaves, sem quina)
      const n = iw * 2
      g.beginPath()
      for (let i = 0; i <= n; i++) {
        const p = base + freq * (i / n) * win
        g.lineTo(x0 + (i / n) * iw, my - waveSample(wave, p) * amp)
      }
      g.stroke()
    } else {
      // denso: envelope min/max com 16 subamostras por coluna
      // (banda exata, sem aliasing), bordas hairline
      const mins = new Float32Array(iw)
      const maxs = new Float32Array(iw)
      for (let i = 0; i < iw; i++) {
        let mn = Infinity
        let mx = -Infinity
        for (let s = 0; s < 16; s++) {
          const v = waveSample(wave, base + freq * (((i + s / 16) / iw) * win))
          if (v < mn) mn = v
          if (v > mx) mx = v
        }
        mins[i] = my - mn * amp
        maxs[i] = my - mx * amp
      }
      g.fillStyle = ph(255, on ? 0.12 : 0.05)
      g.beginPath()
      for (let i = 0; i < iw; i++) g.lineTo(x0 + i, maxs[i])
      for (let i = iw - 1; i >= 0; i--) g.lineTo(x0 + i, mins[i])
      g.closePath()
      g.fill()
      g.beginPath()
      for (let i = 0; i < iw; i++) g.lineTo(x0 + i, maxs[i])
      g.stroke()
      g.beginPath()
      for (let i = 0; i < iw; i++) g.lineTo(x0 + i, mins[i])
      g.stroke()
    }
    g.lineWidth = 1

    // leitura da frequencia em hz, 1 casa decimal
    text(g, freq.toFixed(1), 170, 154, 14, on ? COL.textBright : COL.textDim, 'right')
    text(g, 'HZ', 170, 172, 9, COL.textFaint, 'right')
  },
}
