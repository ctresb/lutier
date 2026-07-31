// terminal de analise ao vivo: porta o renderer do lumiere
// (lumiere/scene.py) pra uma faixa 1920x200 de rodape de stream.
// tudo e desenhado em fosforo cinza (hierarquia = brilho) e o
// gradiente do dono entra so em detalhes (filete do header,
// linha do fmap, goniometro, lut do espectrograma).
//
// o layout e VIVO: um motor de "mood" classifica a musica (CALM /
// GROOVE / PEAK) e os paineis trocam de lugar, tamanho e modo com
// transicao animada. o mesh 3d muda de forma (nuvem de pontos,
// wireframe pulsante, mesh brilhante) conforme o estado.

import { AudioFrame, SPEC_N, WAVE_N, GONIO_N } from './audio'
import { WireMesh } from './glb'
import { ph, grad, gradPaint, gradPaintSoft, STOPS } from './palette'

export const W = 1920
export const H = 200

// ---------- geometria ----------
const FRAME = { x0: 4, y0: 4, x1: 1916, y1: 196 }
const HEADER_Y = 34
const PY0 = 40
const PY1 = 188

// colunas base (slots)
const C = [
  [16, 256], [264, 536], [544, 792], [800, 1120], [1128, 1428], [1436, 1904],
] as const

type Mod = 'input' | 'wave' | 'sgram' | 'entity' | 'mesh' | 'fmap'
type Mood = 'calm' | 'groove' | 'peakspec' | 'peakmesh'
const MODS: Mod[] = ['input', 'wave', 'sgram', 'entity', 'mesh', 'fmap']

// layout por mood: [x0, x1] ou null (modulo some)
const LAYOUTS: Record<Mood, Record<Mod, readonly [number, number] | null>> = {
  groove: {
    input: C[0], wave: C[1], sgram: C[2], entity: C[3], mesh: C[4], fmap: C[5],
  },
  calm: {
    input: C[0], wave: C[1], sgram: C[2], entity: null,
    mesh: [C[3][0], C[4][1]], fmap: C[5],
  },
  peakspec: {
    input: C[0], wave: C[1], sgram: C[2], entity: null, mesh: null,
    fmap: [C[3][0], C[5][1]],
  },
  peakmesh: {
    input: C[0], wave: C[1], sgram: C[2], entity: C[3], mesh: C[4], fmap: C[5],
  },
}

const STATE_LABEL: Record<Mood, string> = {
  calm: 'DRIFTING', groove: 'RESONATING', peakspec: 'SURGING', peakmesh: 'IGNITING',
}
const MESH_TITLE: Record<string, string> = {
  points: 'SUBJECT CLOUD', wire: 'SUBJECT MESH', bright: 'SUBJECT MESH // HOT',
}

const KATA = 'アイウエオカキクケコサシスセソタチツテト0123789'
const HEXCH = '0123456789ABCDEF'

// prng deterministico barato (glifos estaveis por ~125ms)
function mulberry32(seed: number): () => number {
  let a = seed | 0
  return () => {
    a |= 0; a = (a + 0x6d2b79f5) | 0
    let t = Math.imul(a ^ (a >>> 15), 1 | a)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

function gauss(): number {
  return (Math.random() + Math.random() + Math.random()) * 2 - 3
}

const MAXP = 2200
const GONIO_W = 320
const GONIO_H = 150

interface PanelRect { x0: number; x1: number; a: number }

export class Scene {
  private g: CanvasRenderingContext2D
  private staticLayer: HTMLCanvasElement
  private beam: HTMLCanvasElement
  private sgram: HTMLCanvasElement
  private sgramCtx: CanvasRenderingContext2D
  private sgramCol: ImageData
  private sgramLut = new Uint8Array(256 * 3)
  private ribbon: HTMLCanvasElement
  private gonioBuf: HTMLCanvasElement
  private gonioCtx: CanvasRenderingContext2D

  private frame: AudioFrame | null = null
  private frameFresh = false
  private t0 = performance.now()
  private lastT = 0

  // suavizados
  private rmsS = 0
  private loudS = 0
  private fluxS = 0
  private lowS = 0
  private highS = 0
  private fmapSmooth = new Float32Array(SPEC_N)
  private fmapPeaks = new Float32Array(SPEC_N)

  // mood
  private mood: Mood = 'groove'
  private moodAt = 0
  // beat
  private prevLowE = 0
  private beats: number[] = []
  private kick = 0
  private bpm = 0

  // paineis animados
  private rects: Record<Mod, PanelRect>

  private prevLow = 0
  private rings: number[] = []

  // particulas persistentes (mesma mecanica do lumiere)
  private px = new Float32Array(MAXP)
  private py = new Float32Array(MAXP)
  private pvx = new Float32Array(MAXP)
  private pvy = new Float32Array(MAXP)
  private plife = new Float32Array(MAXP)
  private pbri = new Float32Array(MAXP)
  private pcur = 0

  deviceName = 'NO INPUT'
  deviceIdx = 0
  deviceCount = 0
  /** debug: trava o mood (window.__scene.forceMood = 'calm' etc) */
  forceMood: Mood | null = null
  mesh: WireMesh | null = null
  // rotacao acumulada com velocidade suavizada: mudar o alvo de spin
  // nunca teleporta o angulo (era o "brusco" que o dono reclamou)
  private meshRy = 0
  private meshSpin = 0.12

  constructor(canvas: HTMLCanvasElement, private dpr: number) {
    canvas.width = W * dpr
    canvas.height = H * dpr
    const g = canvas.getContext('2d', { alpha: false })
    if (!g) throw new Error('canvas 2d indisponivel')
    this.g = g

    this.rects = {} as Record<Mod, PanelRect>
    for (const m of MODS) {
      const r = LAYOUTS.groove[m]!
      this.rects[m] = { x0: r[0], x1: r[1], a: 1 }
    }

    this.staticLayer = document.createElement('canvas')
    this.staticLayer.width = W * dpr
    this.staticLayer.height = H * dpr
    this.buildStatic()

    // sprite radial do feixe central (estica por frame, custo zero)
    this.beam = document.createElement('canvas')
    this.beam.width = 64
    this.beam.height = 64
    const bg = this.beam.getContext('2d')!
    const rad = bg.createRadialGradient(32, 32, 0, 32, 32, 32)
    rad.addColorStop(0, 'rgba(255,255,255,0.9)')
    rad.addColorStop(0.4, 'rgba(255,255,255,0.32)')
    rad.addColorStop(1, 'rgba(255,255,255,0)')
    bg.fillStyle = rad
    bg.fillRect(0, 0, 64, 64)

    this.sgram = document.createElement('canvas')
    this.sgram.width = 232
    this.sgram.height = PY1 - 66 - 8
    this.sgramCtx = this.sgram.getContext('2d')!
    this.sgramCtx.fillStyle = '#000'
    this.sgramCtx.fillRect(0, 0, this.sgram.width, this.sgram.height)
    this.sgramCol = this.sgramCtx.createImageData(1, this.sgram.height)
    for (let i = 0; i < 256; i++) {
      const u = (i / 255) ** 1.6
      const [r, gg, b] = grad(u)
      const lift = Math.min(1, u * 1.5) ** 0.85
      this.sgramLut[i * 3] = r * lift
      this.sgramLut[i * 3 + 1] = gg * lift
      this.sgramLut[i * 3 + 2] = b * lift
    }

    // fita ciclica do gradiente (loop seamless: paleta + volta)
    this.ribbon = document.createElement('canvas')
    this.ribbon.width = W
    this.ribbon.height = 2
    const rc = this.ribbon.getContext('2d')!
    const cyc = rc.createLinearGradient(0, 0, W, 0)
    const n = STOPS.length
    STOPS.forEach((c, i) => cyc.addColorStop(i / n, c))
    cyc.addColorStop(1, STOPS[0])
    rc.fillStyle = cyc
    rc.fillRect(0, 0, W, 2)

    // buffer de persistencia do goniometro (trilha de fosforo,
    // fundo TRANSPARENTE: nao pode tapar o que esta atras)
    this.gonioBuf = document.createElement('canvas')
    this.gonioBuf.width = GONIO_W
    this.gonioBuf.height = GONIO_H
    this.gonioCtx = this.gonioBuf.getContext('2d')!
  }

  setFrame(f: AudioFrame): void {
    this.frame = f
    this.frameFresh = true
  }

  // ---------- estatico (so a casca) ----------

  private brackets(g: CanvasRenderingContext2D, x0: number, y0: number,
    x1: number, y1: number, len: number, v: number): void {
    g.fillStyle = ph(v)
    for (const [cx, cy, dx, dy] of [
      [x0, y0, 1, 1], [x1, y0, -1, 1], [x0, y1, 1, -1], [x1, y1, -1, -1],
    ]) {
      g.fillRect(Math.min(cx, cx + dx * len), cy - 0.5, len, 1)
      g.fillRect(cx - 0.5, Math.min(cy, cy + dy * len), 1, len)
    }
  }

  private buildStatic(): void {
    const g = this.staticLayer.getContext('2d', { alpha: false })!
    g.setTransform(this.dpr, 0, 0, this.dpr, 0, 0)
    g.fillStyle = '#000'
    g.fillRect(0, 0, W, H)
    g.textBaseline = 'alphabetic'

    // moldura dupla + brackets grandes
    g.strokeStyle = ph(110)
    g.lineWidth = 1
    g.strokeRect(FRAME.x0 + 0.5, FRAME.y0 + 0.5,
      FRAME.x1 - FRAME.x0 - 1, FRAME.y1 - FRAME.y0 - 1)
    g.strokeStyle = ph(45)
    g.strokeRect(FRAME.x0 + 3.5, FRAME.y0 + 3.5,
      FRAME.x1 - FRAME.x0 - 7, FRAME.y1 - FRAME.y0 - 7)
    this.brackets(g, FRAME.x0, FRAME.y0, FRAME.x1, FRAME.y1, 12, 225)

    // linha do header
    g.strokeStyle = ph(95)
    g.beginPath()
    g.moveTo(FRAME.x0, HEADER_Y + 0.5)
    g.lineTo(FRAME.x1, HEADER_Y + 0.5)
    g.stroke()

    // titulo central
    g.font = '12px Lilex, monospace'
    g.fillStyle = ph(200)
    try { (g as unknown as { letterSpacing: string }).letterSpacing = '3px' } catch { /* opcional */ }
    const title = 'LUMIERE :: LIVE SIGNAL MONITOR'
    g.fillText(title, W / 2 - g.measureText(title).width / 2, 25)
    try { (g as unknown as { letterSpacing: string }).letterSpacing = '0px' } catch { /* opcional */ }
  }

  // ---------- mood ----------

  private updateMood(f: AudioFrame, t: number, dt: number): void {
    const db = 20 * Math.log10(Math.max(f.rms, 1e-6))
    const loud = Math.min(Math.max((db + 50) / 40, 0), 1)
    // ataque rapido, release lento (a musica "segura" o estado)
    this.loudS += (loud - this.loudS) * (loud > this.loudS ? 0.2 : 0.03)
    this.fluxS += (f.flux - this.fluxS) * 0.06
    this.lowS += (f.low - this.lowS) * 0.08
    this.highS += (f.high - this.highS) * 0.08
    this.rmsS += (f.rms - this.rmsS) * 0.18

    // beat: onset de grave -> kick + bpm
    const lowE = f.low * f.rms
    if (lowE - this.prevLowE > 0.010 &&
      (!this.beats.length || t - this.beats[this.beats.length - 1] > 0.24)) {
      this.beats.push(t)
      if (this.beats.length > 12) this.beats.shift()
      this.kick = 1
      const iv: number[] = []
      for (let i = 1; i < this.beats.length; i++) {
        const d = this.beats[i] - this.beats[i - 1]
        if (d > 0.25 && d < 1.5) iv.push(d)
      }
      if (iv.length >= 3) {
        iv.sort((a, b) => a - b)
        this.bpm = 60 / iv[Math.floor(iv.length / 2)]
      }
    }
    this.prevLowE += (lowE - this.prevLowE) * 0.28
    this.kick = Math.max(0, this.kick - dt * 3.2)
    if (this.beats.length && t - this.beats[this.beats.length - 1] > 3) this.bpm = 0

    if (this.forceMood) {
      if (this.mood !== this.forceMood) {
        this.mood = this.forceMood
        this.moodAt = t
      }
      return
    }
    const score = this.loudS * 0.55 + Math.min(this.fluxS * 3, 1) * 0.3 +
      this.lowS * 0.15
    const dwell = t - this.moodAt
    const isPeak = this.mood === 'peakspec' || this.mood === 'peakmesh'
    let next: Mood = this.mood
    if (dwell > 5) {
      if (score > 0.62 && !isPeak) {
        // variante do pico: grave dominando = espectro gigante,
        // agudo dominando = mesh incandescente
        next = this.lowS > this.highS * 1.2 ? 'peakspec' : 'peakmesh'
      } else if (score < 0.28 && this.mood !== 'calm') {
        next = 'calm'
      } else if (score >= 0.34 && score <= 0.56 && this.mood !== 'groove') {
        next = 'groove'
      }
    }
    if (next !== this.mood) {
      this.mood = next
      this.moodAt = t
    }
  }

  private updateRects(): void {
    const target = LAYOUTS[this.mood]
    for (const m of MODS) {
      const r = this.rects[m]
      const tr = target[m]
      if (tr) {
        r.a += (1 - r.a) * 0.08
        r.x0 += (tr[0] - r.x0) * 0.1
        r.x1 += (tr[1] - r.x1) * 0.1
      } else {
        r.a += (0 - r.a) * 0.14
      }
    }
  }

  // ---------- chrome de painel (dinamico) ----------

  private panel(g: CanvasRenderingContext2D, r: PanelRect, title: string): void {
    g.strokeStyle = ph(95)
    g.lineWidth = 1
    g.strokeRect(r.x0 + 0.5, PY0 + 0.5, r.x1 - r.x0 - 1, PY1 - PY0 - 1)
    this.brackets(g, r.x0, PY0, r.x1, PY1, 8, 225)
    g.fillStyle = ph(235)
    g.font = '10px Lilex, monospace'
    g.fillText(title, r.x0 + 10, PY0 + 16)
    g.strokeStyle = ph(80)
    g.beginPath()
    g.moveTo(r.x0 + 10, PY0 + 22.5)
    g.lineTo(r.x1 - 10, PY0 + 22.5)
    g.stroke()
  }

  // ---------- header ----------

  private drawHeader(g: CanvasRenderingContext2D, f: AudioFrame, t: number): void {
    g.font = '13px Lilex, monospace'
    g.fillStyle = ph(245)
    g.fillText(`SUBJECT // ${this.deviceName.toUpperCase().slice(0, 30)}`, 18, 25)

    // fita do gradiente em loop seamless (2px, sempre andando)
    const off = Math.floor((t * 42) % W)
    const y = HEADER_Y + 1
    const w = FRAME.x1 - FRAME.x0 - 2
    g.save()
    g.beginPath()
    g.rect(FRAME.x0 + 1, y, w, 2)
    g.clip()
    g.globalAlpha = 0.7
    g.drawImage(this.ribbon, FRAME.x0 + 1 - off, y, W, 2)
    g.drawImage(this.ribbon, FRAME.x0 + 1 - off + W, y, W, 2)
    g.globalAlpha = 1
    g.restore()

    const mm = Math.floor(t / 60) % 60
    const ss = Math.floor(t) % 60
    const fr = Math.floor((t % 1) * 60)
    const hh = Math.floor(t / 3600)
    g.fillStyle = ph(235)
    g.fillText(
      `${String(hh).padStart(2, '0')}:${String(mm).padStart(2, '0')}:` +
      `${String(ss).padStart(2, '0')}:${String(fr).padStart(2, '0')}`,
      1732, 25)
    g.strokeStyle = ph(200)
    g.strokeRect(1848.5, 11.5, 56, 18)
    g.fillStyle = ph(245)
    g.font = '11px Lilex, monospace'
    g.fillText('LIVE', 1868, 24)
    if (Math.floor(t * 2) % 2 === 0) {
      g.fillStyle = ph(255)
      g.beginPath()
      g.arc(1858, 20.5, 3, 0, Math.PI * 2)
      g.fill()
    }
    g.font = '11px Lilex, monospace'
    g.fillStyle = ph(160)
    g.fillText('STATUS:', 560, 25)
    g.fillStyle = ph(240)
    g.fillText(STATE_LABEL[this.mood], 616, 25)
    g.fillStyle = ph(160)
    g.fillText('BPM:', 1580, 25)
    g.fillStyle = ph(240)
    g.fillText(this.bpm > 0 ? this.bpm.toFixed(0).padStart(3, ' ') : '---', 1614, 25)
    // tique de beat: quadradinho que acende no kick
    g.fillStyle = ph(60 + this.kick * 195)
    g.fillRect(1660, 16, 6, 6)
    void f
  }

  // ---------- modulos ----------

  private drawInput(g: CanvasRenderingContext2D, r: PanelRect, f: AudioFrame): void {
    this.panel(g, r, 'INPUT SOURCE')
    const x = r.x0
    g.font = '10px Lilex, monospace'
    g.fillStyle = ph(160)
    g.fillText(
      `${String(this.deviceIdx + 1).padStart(2, '0')}/${String(Math.max(this.deviceCount, 1)).padStart(2, '0')} IN`,
      r.x1 - 58, PY0 + 16)

    g.font = '12px Lilex, monospace'
    g.fillStyle = ph(247)
    let name = this.deviceName.toUpperCase()
    while (name.length > 3 && g.measureText(name).width > 218) name = name.slice(0, -1)
    g.fillText(name, x + 10, 78)
    g.font = '10px Lilex, monospace'
    g.fillStyle = ph(160)
    g.fillText(`SR ${f.sr.toFixed(0)}`, x + 10, 93)
    g.fillText(`CTR ${f.centroid.toFixed(0)}HZ`, x + 110, 93)

    const rmsDb = 20 * Math.log10(Math.max(f.rms, 1e-6))
    const peakDb = 20 * Math.log10(Math.max(f.peak, 1e-6))
    const keys = ['RMS', 'PEAK', 'FLUX', 'CRST', 'WDTH']
    const vals = [
      `${rmsDb.toFixed(1)}`,
      `${peakDb.toFixed(1)}`,
      `${(f.flux * 100).toFixed(1)}%`,
      `${f.crest.toFixed(1)}x`,
      f.width < 0.32 ? 'LOCK' : 'DRFT',
    ]
    const fracs = [
      Math.min(Math.max((rmsDb + 54) / 48, 0), 1),
      Math.min(Math.max((peakDb + 54) / 48, 0), 1),
      Math.min(f.flux * 6, 1),
      Math.min(f.crest / 10, 1),
      f.width < 0.32 ? 0.85 : 0.3,
    ]
    for (let i = 0; i < 5; i++) {
      const y = 108 + i * 15
      g.fillStyle = ph(160)
      g.fillText(keys[i], x + 10, y)
      g.fillStyle = ph(240)
      g.fillText(vals[i], x + 48, y)
      const bx = x + 118
      g.strokeStyle = ph(85)
      g.strokeRect(bx + 0.5, y - 7.5, 110, 7)
      g.fillStyle = ph(200)
      g.fillRect(bx + 1, y - 7, 108 * fracs[i], 6)
    }
    g.fillStyle = ph(105)
    g.font = '8px Lilex, monospace'
    g.fillText('CLICK: NEXT INPUT // RCLICK: PREV', x + 10, 183)
  }

  private drawWave(g: CanvasRenderingContext2D, r: PanelRect, f: AudioFrame): void {
    this.panel(g, r, 'WAVEFORM ANALYSIS')
    const x0 = r.x0 + 10
    const x1 = r.x1 - 10
    const my = (PY0 + PY1) / 2 + 14
    // no pico a onda cresce e clareia
    const boost = 1 + this.kick * 0.5 + (this.mood.startsWith('peak') ? 0.35 : 0)
    const amp = (PY1 - 66) * 0.46 * boost
    g.strokeStyle = ph(55)
    g.beginPath()
    g.moveTo(x0, my + 0.5)
    g.lineTo(x1, my + 0.5)
    g.stroke()
    g.strokeStyle = ph(this.mood.startsWith('peak') ? 250 : 225)
    g.beginPath()
    const wpx = x1 - x0
    for (let k = 0; k <= wpx; k++) {
      const v = f.wave[Math.floor((k / wpx) * (WAVE_N - 1))] / 127
      const y = my - v * amp
      if (k === 0) g.moveTo(x0 + k, y)
      else g.lineTo(x0 + k, y)
    }
    g.stroke()
    g.font = '10px Lilex, monospace'
    g.fillStyle = ph(200)
    g.fillText(`Δ ${f.rms.toFixed(4)}`, r.x1 - 78, PY0 + 16)
  }

  private drawSgram(g: CanvasRenderingContext2D, r: PanelRect, f: AudioFrame): void {
    this.panel(g, r, 'SPECTROGRAM')
    const sc = this.sgramCtx
    const w = this.sgram.width
    const h = this.sgram.height
    if (this.frameFresh) {
      sc.drawImage(this.sgram, -1, 0)
      const col = this.sgramCol
      for (let y = 0; y < h; y++) {
        const bin = Math.floor((1 - y / (h - 1)) * (SPEC_N - 1))
        const v = f.spec[bin]
        const i = y * 4
        col.data[i] = this.sgramLut[v * 3]
        col.data[i + 1] = this.sgramLut[v * 3 + 1]
        col.data[i + 2] = this.sgramLut[v * 3 + 2]
        col.data[i + 3] = 255
      }
      sc.putImageData(col, w - 1, 0)
    }
    const dw = r.x1 - r.x0 - 16
    g.drawImage(this.sgram, r.x0 + 8, 66, dw, h)
    g.fillStyle = ph(140)
    g.fillRect(r.x0 + 8 + dw - 1, 66, 1, h)
  }

  private spawnParticles(r: PanelRect, f: AudioFrame, dt: number): void {
    const cx = (r.x0 + r.x1) / 2
    const top = 66
    const bot = PY1 - 12
    let sum = 0
    const en = new Float32Array(SPEC_N)
    for (let i = 0; i < SPEC_N; i++) {
      en[i] = (f.spec[i] / 255) ** 1.4
      sum += en[i]
    }
    if (sum < 1e-4) return
    const n = Math.min(140, Math.floor((10 + sum * 2.6) * (dt * 60)))
    for (let k = 0; k < n; k++) {
      let bin = 0
      for (let tries = 0; tries < 8; tries++) {
        bin = Math.floor(Math.random() * SPEC_N)
        if (Math.random() < en[bin] / (sum / SPEC_N + 1e-6) * 0.25) break
      }
      const v = f.spec[bin] / 255
      const hw = 6 + v * 66
      let x = cx + gauss() * hw * 0.42
      if (Math.random() < 0.5) x = 2 * cx - x
      const i = this.pcur
      this.pcur = (this.pcur + 1) % MAXP
      this.px[i] = Math.min(Math.max(x, r.x0 + 6), r.x1 - 6)
      this.py[i] = bot - (bin / SPEC_N) * (bot - top) + gauss() * 3
      this.pvx[i] = gauss() * 1.6
      this.pvy[i] = -6 - Math.random() * 16
      this.plife[i] = 0.35 + Math.random()
      this.pbri[i] = 90 + v * 150
    }
  }

  private drawEntity(g: CanvasRenderingContext2D, r: PanelRect, f: AudioFrame,
    t: number, dt: number): void {
    this.panel(g, r, 'ENTITY // GONIO')
    const cx = (r.x0 + r.x1) / 2
    const cy = (PY0 + PY1) / 2 + 8
    const bot = PY1 - 12

    g.save()
    g.beginPath()
    g.rect(r.x0 + 4, PY0 + 26, r.x1 - r.x0 - 8, PY1 - PY0 - 32)
    g.clip()

    // grade de pontos
    g.fillStyle = ph(26)
    for (let y = 74; y < PY1 - 10; y += 23) {
      for (let x = r.x0 + 16; x < r.x1 - 10; x += 23) {
        g.fillRect(x, y, 1, 1)
      }
    }

    // feixe central respirando com o rms
    const bw = 10 + f.low * 56
    const bh = bot - 62
    g.globalAlpha = Math.min(this.rmsS * 3.4, 0.85)
    g.drawImage(this.beam, cx - bw, 62, bw * 2, bh * 1.9)
    g.globalAlpha = 1

    // aneis de grave
    const low = f.low * f.rms
    if (low - this.prevLow > 0.008 &&
      (!this.rings.length || t - this.rings[this.rings.length - 1] > 0.18)) {
      this.rings.push(t)
    }
    this.prevLow = low
    this.rings = this.rings.filter((rr) => t - rr < 1.2)
    g.setLineDash([5, 9])
    for (const r0 of this.rings) {
      const age = t - r0
      const rr = 12 + age * 130
      const fade = Math.max(0, 110 * (1 - age / 1.2))
      if (fade < 5) continue
      g.strokeStyle = ph(fade)
      g.beginPath()
      g.ellipse(cx, bot - 22, rr * 1.7, rr * 0.4, 0, 0, Math.PI * 2)
      g.stroke()
    }
    g.setLineDash([])

    // particulas
    this.spawnParticles(r, f, dt)
    const buckets: number[][] = [[], [], [], [], [], []]
    for (let i = 0; i < MAXP; i++) {
      if (this.plife[i] <= 0) continue
      this.plife[i] -= dt
      this.px[i] += this.pvx[i] * dt
      this.py[i] += this.pvy[i] * dt
      if (this.plife[i] <= 0) continue
      const x = this.px[i]
      const y = this.py[i]
      if (x < r.x0 + 4 || x > r.x1 - 4 || y < 62 || y > PY1 - 6) continue
      const b = this.pbri[i] * Math.min(this.plife[i] / 0.4, 1)
      buckets[Math.min(5, Math.floor(b / 43))].push(x, y)
    }
    for (let k = 1; k < 6; k++) {
      g.fillStyle = ph(43 * k + 21)
      const pts = buckets[k]
      for (let j = 0; j < pts.length; j += 2) {
        g.fillRect(pts[j], pts[j + 1], 1.1, 1.1)
      }
    }

    // ---- goniometro: nuvem suave com persistencia longa ----
    const gc = this.gonioCtx
    // fade lento da trilha (fosforo apagando devagar = suave)
    gc.globalCompositeOperation = 'destination-out'
    gc.fillStyle = 'rgba(0,0,0,0.085)'
    gc.fillRect(0, 0, GONIO_W, GONIO_H)
    gc.globalCompositeOperation = 'source-over'
    const gx0 = GONIO_W / 2
    const gy0 = GONIO_H / 2
    const S = 62
    // cor pastel (gradiente puxado pro branco de fosforo)
    const paint = gradPaintSoft(gc, gx0 - S, gx0 + S, 0.5)
    // traco fino ligando as ultimas amostras (lissajous vivo)
    gc.strokeStyle = paint
    gc.globalAlpha = 0.12
    gc.lineWidth = 0.8
    gc.beginPath()
    for (let i = GONIO_N - 160; i < GONIO_N; i++) {
      const x = gx0 + (f.gonio[i * 2] / 127) * S
      const y = gy0 - (f.gonio[i * 2 + 1] / 127) * S * 0.72
      if (i === GONIO_N - 160) gc.moveTo(x, y)
      else gc.lineTo(x, y)
    }
    gc.stroke()
    // pontos soft: halo 2x2 fraquinho + nucleo 1x1
    gc.fillStyle = paint
    for (let pass = 0; pass < 4; pass++) {
      const start = Math.floor((pass / 4) * GONIO_N)
      const end = Math.floor(((pass + 1) / 4) * GONIO_N)
      gc.globalAlpha = 0.05 + pass * 0.04
      for (let i = start; i < end; i++) {
        const x = gx0 + (f.gonio[i * 2] / 127) * S
        const y = gy0 - (f.gonio[i * 2 + 1] / 127) * S * 0.72
        gc.fillRect(x - 1, y - 1, 2.4, 2.4)
      }
      gc.globalAlpha = 0.1 + pass * 0.08
      for (let i = start; i < end; i++) {
        const x = gx0 + (f.gonio[i * 2] / 127) * S
        const y = gy0 - (f.gonio[i * 2 + 1] / 127) * S * 0.72
        gc.fillRect(x, y, 1, 1)
      }
    }
    gc.globalAlpha = 1
    const dw = Math.min(GONIO_W, r.x1 - r.x0 - 24)
    const dx = cx - dw / 2
    const dy = cy - GONIO_H / 2 + 6
    g.drawImage(this.gonioBuf, 0, 0, GONIO_W, GONIO_H, dx, dy, dw, GONIO_H)

    // guia: anel + eixos LR
    g.strokeStyle = ph(44)
    g.beginPath()
    g.ellipse(cx, cy + 6, S * 0.9, S * 0.66, 0, 0, Math.PI * 2)
    g.stroke()
    g.strokeStyle = ph(36)
    g.beginPath()
    g.moveTo(cx - 52, cy - 46); g.lineTo(cx + 52, cy + 58)
    g.moveTo(cx + 52, cy - 46); g.lineTo(cx - 52, cy + 58)
    g.stroke()
    g.font = '8px Lilex, monospace'
    g.fillStyle = ph(105)
    g.fillText('L', cx - 62, cy - 48)
    g.fillText('R', cx + 56, cy - 48)

    // correlacao (largura estereo) como reguinha embaixo
    const corr = 1 - f.width * 2
    const cw = 84
    g.strokeStyle = ph(70)
    g.strokeRect(cx - cw / 2 + 0.5, PY1 - 18.5, cw, 5)
    g.fillStyle = ph(210)
    const cpos = cx + (corr * cw) / 2
    g.fillRect(cpos - 1, PY1 - 19, 2, 6)
    g.fillStyle = ph(105)
    g.fillText('-1', cx - cw / 2 - 14, PY1 - 13)
    g.fillText('+1', cx + cw / 2 + 4, PY1 - 13)

    g.restore()

    // coordenadas
    g.font = '9px Lilex, monospace'
    g.fillStyle = ph(160)
    g.fillText('X', r.x0 + 10, 76)
    g.fillText('Y', r.x0 + 10, 88)
    g.fillText('Z', r.x0 + 10, 100)
    g.fillStyle = ph(235)
    g.fillText(f.centroid.toFixed(2).padStart(8, ' '), r.x0 + 20, 76)
    g.fillText((f.rms * 1000).toFixed(2).padStart(8, ' '), r.x0 + 20, 88)
    g.fillText((f.width * 100).toFixed(2).padStart(8, ' '), r.x0 + 20, 100)

    // glifos flutuantes
    const rng = mulberry32(Math.floor(t * 8) * 7919)
    const ng = Math.floor(3 + f.high * 26)
    g.font = '9px Lilex, monospace'
    for (let k = 0; k < ng; k++) {
      const gx = r.x0 + 14 + rng() * (r.x1 - r.x0 - 30)
      const gy = 72 + rng() * (PY1 - 88)
      if (Math.abs(gx - cx) < 66) continue
      const ch = rng() < 0.75
        ? HEXCH[Math.floor(rng() * HEXCH.length)]
        : KATA[Math.floor(rng() * KATA.length)]
      g.fillStyle = ph(50 + rng() * 110)
      g.fillText(ch, gx, gy)
    }
  }

  private meshMode(): 'points' | 'wire' | 'bright' {
    if (this.mood === 'calm') return 'points'
    if (this.mood === 'peakmesh') return 'bright'
    return 'wire'
  }

  private drawMesh(g: CanvasRenderingContext2D, r: PanelRect, f: AudioFrame,
    t: number, dt: number): void {
    const mode = this.meshMode()
    this.panel(g, r, MESH_TITLE[mode])
    const m = this.mesh
    const cx = (r.x0 + r.x1) / 2
    const cy = PY1 - 22
    if (!m) {
      g.font = '10px Lilex, monospace'
      g.fillStyle = ph(105)
      g.fillText('MESH: LOADING...', r.x0 + 10, (PY0 + PY1) / 2)
      return
    }
    // rotacao SUAVE: velocidade alvo por modo, interpolada devagar,
    // angulo acumulado (nunca salta)
    const spinTarget = mode === 'points' ? 0.08 : 0.12 + Math.min(this.fluxS, 0.4) * 0.2
    this.meshSpin += (spinTarget - this.meshSpin) * 0.02
    this.meshRy += this.meshSpin * dt
    const ry = this.meshRy
    const rx = -0.34 + Math.sin(t * 0.13) * 0.06
    // pulso: respiro + chute do beat
    const pulse = 1 + this.rmsS * 0.25 + this.kick * (mode === 'bright' ? 0.13 : 0.06)
    const wide = r.x1 - r.x0 > 400
    const scale = (wide ? 560 : 500) * pulse
    const cyr = Math.cos(ry); const syr = Math.sin(ry)
    const cxr = Math.cos(rx); const sxr = Math.sin(rx)
    const camz = 2.4
    const n = m.verts.length / 3
    const sx = new Float32Array(n)
    const sy = new Float32Array(n)
    const sz = new Float32Array(n)
    for (let i = 0; i < n; i++) {
      const x = m.verts[i * 3]
      const y = m.verts[i * 3 + 1]
      const z = m.verts[i * 3 + 2]
      const x1 = x * cyr + z * syr
      const z1 = -x * syr + z * cyr
      const y2 = y * cxr - z1 * sxr
      const z2 = y * sxr + z1 * cxr
      const d = camz - z2
      sx[i] = cx + (x1 / d) * scale
      sy[i] = cy - (y2 / d) * scale
      sz[i] = z2
    }
    g.save()
    g.beginPath()
    g.rect(r.x0 + 4, PY0 + 26, r.x1 - r.x0 - 8, PY1 - PY0 - 32)
    g.clip()

    if (mode === 'points') {
      // nuvem de pontos: so os vertices, cintilando de leve
      const rng = mulberry32(Math.floor(t * 4) * 131)
      for (let i = 0; i < n; i++) {
        const depth = Math.min(Math.max((sz[i] + 0.55) / 1.1, 0), 1)
        const tw = rng() < 0.03 ? 90 : 0
        g.fillStyle = ph(70 + depth * 120 + tw, 0.28 + depth * 0.4)
        g.fillRect(sx[i], sy[i], 1.2, 1.2)
      }
    } else {
      // mesh INTEIRA: a metade de baixo simplesmente fica fora do
      // enquadramento (clip do painel), nada de geometria cortada
      const edges = m.edges
      const ne = edges.length / 2
      const paths: Path2D[] = [new Path2D(), new Path2D(), new Path2D(), new Path2D()]
      for (let i = 0; i < ne; i++) {
        const a = edges[i * 2]
        const b = edges[i * 2 + 1]
        const z = (sz[a] + sz[b]) * 0.5
        const bucket = Math.min(3, Math.max(0, Math.floor((z + 0.55) * 3.6)))
        paths[bucket].moveTo(sx[a], sy[a])
        paths[bucket].lineTo(sx[b], sy[b])
      }
      const hot = mode === 'bright'
      g.lineWidth = hot ? 0.8 : 0.55
      const kickA = this.kick * 0.2
      const alphas = hot
        ? [0.1 + kickA, 0.2 + kickA, 0.38 + kickA, 0.66 + kickA]
        : [0.05, 0.11, 0.2, 0.36]
      const bris = hot ? [150, 185, 225, 255] : [120, 150, 185, 225]
      for (let k = 0; k < 4; k++) {
        g.strokeStyle = ph(bris[k], Math.min(alphas[k], 1))
        g.stroke(paths[k])
      }
    }
    g.restore()

    g.font = '9px Lilex, monospace'
    g.fillStyle = ph(160)
    const deg = ((ry * 180 / Math.PI) % 360 + 360) % 360
    g.fillText(`RY ${deg.toFixed(1).padStart(5, '0')}°`, r.x1 - 70, PY0 + 16)
    g.fillStyle = ph(105)
    const ne2 = m.edges.length / 2
    g.fillText(mode === 'points' ? `VTX ${n}` : `VTX ${n} EDG ${ne2}`,
      r.x1 - 118, 183)
    g.fillText('/SUBJECT_MESH.GLB', r.x0 + 10, 183)
    void f
  }

  private drawFmap(g: CanvasRenderingContext2D, r: PanelRect, f: AudioFrame,
    dt: number): void {
    const wide = r.x1 - r.x0 > 600
    this.panel(g, r, wide ? 'FREQUENCY MAP // FULL' : 'FREQUENCY MAP')
    const x0 = r.x0 + 10
    const x1 = r.x1 - (wide ? 20 : 96)
    const y0 = 66
    const y1 = PY1 - 16
    const wpx = x1 - x0

    // espectro suavizado (chill alisa mais) + peak hold
    const k = this.mood === 'calm' ? 0.12 : 0.4
    for (let i = 0; i < SPEC_N; i++) {
      const v = f.spec[i] / 255
      this.fmapSmooth[i] += (v - this.fmapSmooth[i]) * k
      if (v > this.fmapPeaks[i]) this.fmapPeaks[i] = v
      else this.fmapPeaks[i] = Math.max(0, this.fmapPeaks[i] - dt * 0.24)
    }

    if (wide) {
      // modo largo: barras verticais com fade de opacidade (100% na
      // base -> 0% no topo) + peak-hold caindo (so aqui, regra do dono)
      const nb = 96
      const bw = wpx / nb
      for (let b = 0; b < nb; b++) {
        let v = 0
        let pk = 0
        const i0 = Math.floor((b / nb) * SPEC_N)
        const i1 = Math.max(i0 + 1, Math.floor(((b + 1) / nb) * SPEC_N))
        for (let i = i0; i < i1; i++) {
          v = Math.max(v, this.fmapSmooth[i])
          pk = Math.max(pk, this.fmapPeaks[i])
        }
        const bx = x0 + b * bw
        const bh = Math.max(v * (y1 - y0), 1)
        const [cr, cg, cb] = grad(b / nb)
        const ramp = g.createLinearGradient(0, y1, 0, y1 - bh)
        ramp.addColorStop(0, `rgba(${cr | 0},${cg | 0},${cb | 0},${0.55 + v * 0.45})`)
        ramp.addColorStop(1, `rgba(${cr | 0},${cg | 0},${cb | 0},0)`)
        g.fillStyle = ramp
        g.fillRect(bx, y1 - bh, bw - 1.5, bh)
        // tampa de peak-hold
        g.fillStyle = ph(235)
        g.fillRect(bx, y1 - pk * (y1 - y0) - 2, bw - 1.5, 1.2)
      }
    } else {
      // modo estreito: SO a linha (sem peak-hold, sem barra)
      g.strokeStyle = gradPaint(g, x0, x1)
      g.lineWidth = 1
      g.beginPath()
      const tops = new Float32Array(wpx + 1)
      for (let kk = 0; kk <= wpx; kk++) {
        const v = this.fmapSmooth[Math.floor((kk / wpx) * (SPEC_N - 1))]
        const y = y1 - v * (y1 - y0)
        tops[kk] = y
        if (kk === 0) g.moveTo(x0 + kk, y)
        else g.lineTo(x0 + kk, y)
      }
      g.stroke()
      if (this.mood !== 'calm') {
        g.fillStyle = ph(60)
        for (let kk = 0; kk < wpx; kk += 4) {
          for (let yy = tops[kk] + 4; yy < y1 - 2; yy += 6) {
            g.fillRect(x0 + kk, yy, 1, 1)
          }
        }
      }
    }

    if (!wide) {
      const bands = [f.low, f.mid, f.high]
      const lbls = ['LOW', 'MID', 'HI ']
      g.font = '10px Lilex, monospace'
      for (let b = 0; b < 3; b++) {
        g.fillStyle = ph(160)
        g.fillText(lbls[b], r.x1 - 84, 80 + b * 18)
        g.fillStyle = ph(210)
        g.fillText(`Δ ${(bands[b] * 99).toFixed(1).padStart(4, '0')}`, r.x1 - 58, 80 + b * 18)
      }
      g.fillStyle = ph(130)
      g.fillText('...', r.x1 - 84, 80 + 3 * 18)
    }
    g.font = '8px Lilex, monospace'
    g.fillStyle = ph(105)
    const ticks = ['31', '125', '500', '2K', '8K']
    ticks.forEach((lbl, i) => {
      g.fillText(lbl, x0 + 2 + i * (wpx / 5), 184)
    })
  }

  // ---------- frame ----------

  render(now: number): void {
    const t = (now - this.t0) / 1000
    const dt = Math.min(0.1, this.lastT ? t - this.lastT : 1 / 60)
    this.lastT = t
    const g = this.g
    g.setTransform(this.dpr, 0, 0, this.dpr, 0, 0)
    g.textBaseline = 'alphabetic'
    g.drawImage(this.staticLayer, 0, 0, W, H)

    const f = this.frame ?? emptyFrame()
    this.updateMood(f, t, dt)
    this.updateRects()

    this.drawHeader(g, f, t)

    const draw: Record<Mod, () => void> = {
      input: () => this.drawInput(g, this.rects.input, f),
      wave: () => this.drawWave(g, this.rects.wave, f),
      sgram: () => this.drawSgram(g, this.rects.sgram, f),
      entity: () => this.drawEntity(g, this.rects.entity, f, t, dt),
      mesh: () => this.drawMesh(g, this.rects.mesh, f, t, dt),
      fmap: () => this.drawFmap(g, this.rects.fmap, f, dt),
    }
    for (const m of MODS) {
      const r = this.rects[m]
      if (r.a < 0.03) continue
      g.globalAlpha = Math.min(r.a, 1)
      draw[m]()
      g.globalAlpha = 1
    }
    this.frameFresh = false
  }

  /** painel de input em coords de cena (pro hit test do clique) */
  inputPanelHit(x: number, y: number): boolean {
    const r = this.rects.input
    return x >= r.x0 && x <= r.x1 && y >= PY0 && y <= PY1
  }
}

function emptyFrame(): AudioFrame {
  return {
    spec: new Uint8Array(SPEC_N),
    wave: new Int8Array(WAVE_N),
    gonio: new Int8Array(GONIO_N * 2),
    rms: 0, peak: 0, centroid: 0, flux: 0, crest: 0, width: 0,
    low: 0, mid: 0, high: 0, sr: 0,
  }
}
