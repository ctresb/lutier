// terminal de analise ao vivo: porta o renderer do lumiere
// (lumiere/scene.py) pra uma faixa 1920x200 de rodape de stream.
// tudo e desenhado em fosforo cinza (hierarquia = brilho) e a matiz
// entra numa unica passada de blend 'color' com o gradiente do dono.

import { AudioFrame, SPEC_N, WAVE_N, GONIO_N } from './audio'
import { WireMesh } from './glb'
import { ph, grad, gradPaint } from './palette'

export const W = 1920
export const H = 200

// ---------- geometria ----------
const FRAME = { x0: 4, y0: 4, x1: 1916, y1: 196 }
const HEADER_Y = 34
const P = {
  input: { x0: 16, x1: 256, title: 'INPUT SOURCE' },
  wave: { x0: 264, x1: 536, title: 'WAVEFORM ANALYSIS' },
  sgram: { x0: 544, x1: 792, title: 'SPECTROGRAM' },
  entity: { x0: 800, x1: 1120, title: 'ENTITY // GONIO' },
  mesh: { x0: 1128, x1: 1428, title: 'SUBJECT MESH' },
  fmap: { x0: 1436, x1: 1904, title: 'FREQUENCY MAP' },
}
const PY0 = 40
const PY1 = 188

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

export class Scene {
  private g: CanvasRenderingContext2D
  private staticLayer: HTMLCanvasElement
  private beam: HTMLCanvasElement
  private sgram: HTMLCanvasElement
  private sgramCtx: CanvasRenderingContext2D
  private sgramCol: ImageData
  // lut do espectrograma: intensidade percorre o gradiente do dono
  // (fraco = quase preto, forte = sobe a paleta ate o amarelo)
  private sgramLut = new Uint8Array(256 * 3)

  private frame: AudioFrame | null = null
  private frameFresh = false
  private t0 = performance.now()
  private rmsS = 0
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
  private lastT = 0

  deviceName = 'NO INPUT'
  deviceIdx = 0
  deviceCount = 0
  mesh: WireMesh | null = null
  // so a metade de cima do corpo (pedido do dono): arestas com os
  // dois vertices acima da cintura (y >= 0 no modelo normalizado)
  private upperEdges: Uint32Array | null = null

  constructor(canvas: HTMLCanvasElement, private dpr: number) {
    canvas.width = W * dpr
    canvas.height = H * dpr
    const g = canvas.getContext('2d', { alpha: false })
    if (!g) throw new Error('canvas 2d indisponivel')
    this.g = g

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

    const sg = P.sgram
    this.sgram = document.createElement('canvas')
    this.sgram.width = sg.x1 - sg.x0 - 16
    this.sgram.height = PY1 - 66 - 8
    this.sgramCtx = this.sgram.getContext('2d', { willReadFrequently: false })!
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
  }

  setFrame(f: AudioFrame): void {
    this.frame = f
    this.frameFresh = true
  }

  // ---------- estatico ----------

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

  private panel(g: CanvasRenderingContext2D, x0: number, x1: number,
    title: string): void {
    g.strokeStyle = ph(95)
    g.lineWidth = 1
    g.strokeRect(x0 + 0.5, PY0 + 0.5, x1 - x0 - 1, PY1 - PY0 - 1)
    this.brackets(g, x0, PY0, x1, PY1, 8, 225)
    g.fillStyle = ph(235)
    g.font = '10px Lilex, monospace'
    g.fillText(title, x0 + 10, PY0 + 16)
    g.strokeStyle = ph(80)
    g.beginPath()
    g.moveTo(x0 + 10, PY0 + 22.5)
    g.lineTo(x1 - 10, PY0 + 22.5)
    g.stroke()
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

    // linha do header + filete de gradiente (o unico acento de cor
    // que atravessa a faixa; regra do dono: cor so em detalhe sutil)
    g.strokeStyle = ph(95)
    g.beginPath()
    g.moveTo(FRAME.x0, HEADER_Y + 0.5)
    g.lineTo(FRAME.x1, HEADER_Y + 0.5)
    g.stroke()
    g.globalAlpha = 0.55
    g.fillStyle = gradPaint(g, FRAME.x0, FRAME.x1)
    g.fillRect(FRAME.x0 + 1, HEADER_Y + 1, FRAME.x1 - FRAME.x0 - 2, 1)
    g.globalAlpha = 1

    // titulo central
    g.font = '12px Lilex, monospace'
    g.fillStyle = ph(200)
    try { (g as unknown as { letterSpacing: string }).letterSpacing = '3px' } catch { /* opcional */ }
    const title = 'LUMIERE :: LIVE SIGNAL MONITOR'
    g.fillText(title, W / 2 - g.measureText(title).width / 2, 25)
    try { (g as unknown as { letterSpacing: string }).letterSpacing = '0px' } catch { /* opcional */ }

    // paineis
    for (const p of Object.values(P)) this.panel(g, p.x0, p.x1, p.title)

    // labels estaticos do input
    const ix = P.input.x0
    g.font = '10px Lilex, monospace'
    g.fillStyle = ph(160)
    const keys = ['RMS', 'PEAK', 'FLUX', 'CRST', 'WDTH']
    keys.forEach((k, i) => g.fillText(k, ix + 10, 108 + i * 15))
    g.fillStyle = ph(105)
    g.font = '8px Lilex, monospace'
    g.fillText('CLICK: NEXT INPUT // RCLICK: PREV', ix + 10, 183)

    // grade de pontos do palco da entidade
    const e = P.entity
    g.fillStyle = ph(26)
    for (let y = 74; y < PY1 - 10; y += 23) {
      for (let x = e.x0 + 16; x < e.x1 - 10; x += 23) {
        g.fillRect(x, y, 1, 1)
      }
    }

    // eixos do goniometro (cruz diagonal sutil)
    const cx = (e.x0 + e.x1) / 2
    const cy = (PY0 + PY1) / 2 + 8
    g.strokeStyle = ph(40)
    g.beginPath()
    g.moveTo(cx - 52, cy - 52); g.lineTo(cx + 52, cy + 52)
    g.moveTo(cx + 52, cy - 52); g.lineTo(cx - 52, cy + 52)
    g.stroke()
    g.fillStyle = ph(105)
    g.font = '8px Lilex, monospace'
    g.fillText('L', cx - 62, cy - 54)
    g.fillText('R', cx + 56, cy - 54)

    // labels do mesh
    const m = P.mesh
    g.fillStyle = ph(105)
    g.fillText('/SUBJECT_MESH.GLB', m.x0 + 10, 183)

    // regua de frequencia do mapa
    const f = P.fmap
    g.fillStyle = ph(105)
    ;['31', '125', '500', '2K', '8K'].forEach((lbl, i) => {
      g.fillText(lbl, f.x0 + 12 + i * ((f.x1 - f.x0 - 110) / 5), 184)
    })
  }

  // ---------- dinamico ----------

  private drawHeader(g: CanvasRenderingContext2D, t: number): void {
    g.font = '13px Lilex, monospace'
    g.fillStyle = ph(245)
    g.fillText(`SUBJECT // ${this.deviceName.toUpperCase().slice(0, 30)}`, 18, 25)

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
    g.fillStyle = ph(180)
    g.font = '11px Lilex, monospace'
    g.fillText('STATUS: RESONATING', 620, 25)
    g.fillText('LINK FEED: LIVE', 1580, 25)
  }

  private drawInput(g: CanvasRenderingContext2D, f: AudioFrame): void {
    const x = P.input.x0
    g.font = '10px Lilex, monospace'
    g.fillStyle = ph(160)
    g.fillText(
      `${String(this.deviceIdx + 1).padStart(2, '0')}/${String(Math.max(this.deviceCount, 1)).padStart(2, '0')} IN`,
      P.input.x1 - 58, PY0 + 16)

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
      g.fillStyle = ph(240)
      g.fillText(vals[i], x + 48, y)
      const bx = x + 118
      g.strokeStyle = ph(85)
      g.strokeRect(bx + 0.5, y - 7.5, 110, 7)
      g.fillStyle = ph(200)
      g.fillRect(bx + 1, y - 7, 108 * fracs[i], 6)
    }
  }

  private drawWave(g: CanvasRenderingContext2D, f: AudioFrame): void {
    const p = P.wave
    const x0 = p.x0 + 10
    const x1 = p.x1 - 10
    const my = (PY0 + PY1) / 2 + 14
    const amp = (PY1 - 66) * 0.46
    g.strokeStyle = ph(55)
    g.beginPath()
    g.moveTo(x0, my + 0.5)
    g.lineTo(x1, my + 0.5)
    g.stroke()
    g.strokeStyle = ph(225)
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
    g.fillText(`Δ ${f.rms.toFixed(4)}`, p.x1 - 78, PY0 + 16)
  }

  private drawSgram(g: CanvasRenderingContext2D, f: AudioFrame): void {
    const p = P.sgram
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
    g.drawImage(this.sgram, p.x0 + 8, 66)
    // cursor de tempo (borda direita)
    g.fillStyle = ph(140)
    g.fillRect(p.x0 + 8 + w - 1, 66, 1, h)
  }

  private spawnParticles(f: AudioFrame, dt: number): void {
    const e = P.entity
    const cx = (e.x0 + e.x1) / 2
    const top = 66
    const bot = PY1 - 12
    // energia por bin -> quantidade e distribuicao das particulas
    let sum = 0
    const en = new Float32Array(SPEC_N)
    for (let i = 0; i < SPEC_N; i++) {
      en[i] = (f.spec[i] / 255) ** 1.4
      sum += en[i]
    }
    if (sum < 1e-4) return
    const n = Math.min(140, Math.floor((10 + sum * 2.6) * (dt * 60)))
    for (let k = 0; k < n; k++) {
      // amostragem por rejeicao barata (spec e denso, converge rapido)
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
      this.px[i] = Math.min(Math.max(x, e.x0 + 6), e.x1 - 6)
      this.py[i] = bot - (bin / SPEC_N) * (bot - top) + gauss() * 3
      this.pvx[i] = gauss() * 1.6
      this.pvy[i] = -6 - Math.random() * 16
      this.plife[i] = 0.35 + Math.random()
      this.pbri[i] = 90 + v * 150
    }
  }

  private drawEntity(g: CanvasRenderingContext2D, f: AudioFrame,
    t: number, dt: number): void {
    const e = P.entity
    const cx = (e.x0 + e.x1) / 2
    const cy = (PY0 + PY1) / 2 + 8
    const bot = PY1 - 12

    // nada vaza do outline do painel
    g.save()
    g.beginPath()
    g.rect(e.x0 + 4, PY0 + 26, e.x1 - e.x0 - 8, PY1 - PY0 - 32)
    g.clip()

    // feixe central respirando com o rms
    const bw = 10 + f.low * 56
    const bh = bot - 62
    g.globalAlpha = Math.min(this.rmsS * 3.4, 0.85)
    g.drawImage(this.beam, cx - bw, 62, bw * 2, bh * 1.9)
    g.globalAlpha = 1

    // aneis de grave (pulsos elipticos)
    const low = f.low * f.rms
    if (low - this.prevLow > 0.008 &&
      (!this.rings.length || t - this.rings[this.rings.length - 1] > 0.18)) {
      this.rings.push(t)
    }
    this.prevLow = low
    this.rings = this.rings.filter((r) => t - r < 1.2)
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
    this.spawnParticles(f, dt)
    const buckets: number[][] = [[], [], [], [], [], []]
    for (let i = 0; i < MAXP; i++) {
      if (this.plife[i] <= 0) continue
      this.plife[i] -= dt
      this.px[i] += this.pvx[i] * dt
      this.py[i] += this.pvy[i] * dt
      if (this.plife[i] <= 0) continue
      const x = this.px[i]
      const y = this.py[i]
      if (x < e.x0 + 4 || x > e.x1 - 4 || y < 62 || y > PY1 - 6) continue
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

    // goniometro: nuvem de pontinhos no centro. acento de cor sutil:
    // os pontos pegam a matiz do gradiente pela posicao x
    const S = 54
    const gonioPaint = gradPaint(g, cx - S, cx + S)
    g.fillStyle = gonioPaint
    for (let pass = 0; pass < 4; pass++) {
      g.globalAlpha = 0.28 + pass * 0.18
      const start = Math.floor((pass / 4) * GONIO_N)
      const end = Math.floor(((pass + 1) / 4) * GONIO_N)
      for (let i = start; i < end; i++) {
        const gx = cx + (f.gonio[i * 2] / 127) * S
        const gy = cy - (f.gonio[i * 2 + 1] / 127) * S * 0.82
        g.fillRect(gx, gy, 1, 1)
      }
    }
    g.globalAlpha = 1

    g.restore()

    // coordenadas
    g.font = '9px Lilex, monospace'
    g.fillStyle = ph(160)
    g.fillText('X', e.x0 + 10, 76)
    g.fillText('Y', e.x0 + 10, 88)
    g.fillText('Z', e.x0 + 10, 100)
    g.fillStyle = ph(235)
    g.fillText(f.centroid.toFixed(2).padStart(8, ' '), e.x0 + 20, 76)
    g.fillText((f.rms * 1000).toFixed(2).padStart(8, ' '), e.x0 + 20, 88)
    g.fillText((f.width * 100).toFixed(2).padStart(8, ' '), e.x0 + 20, 100)

    // glifos flutuantes (hex + katakana), estaveis por 125ms
    const rng = mulberry32(Math.floor(t * 8) * 7919)
    const ng = Math.floor(3 + f.high * 26)
    g.font = '9px Lilex, monospace'
    for (let k = 0; k < ng; k++) {
      const gx = e.x0 + 14 + rng() * (e.x1 - e.x0 - 30)
      const gy = 72 + rng() * (PY1 - 88)
      if (Math.abs(gx - cx) < 66) continue
      const ch = rng() < 0.75
        ? HEXCH[Math.floor(rng() * HEXCH.length)]
        : KATA[Math.floor(rng() * KATA.length)]
      g.fillStyle = ph(50 + rng() * 110)
      g.fillText(ch, gx, gy)
    }
  }

  private drawMesh(g: CanvasRenderingContext2D, f: AudioFrame, t: number): void {
    const p = P.mesh
    const m = this.mesh
    const cx = (p.x0 + p.x1) / 2
    // so a metade de cima aparece, entao a "cintura" senta perto do
    // rodape do painel e o busto ocupa o espaco todo
    const cy = PY1 - 22
    if (!m) {
      g.font = '10px Lilex, monospace'
      g.fillStyle = ph(105)
      g.fillText('MESH: LOADING...', p.x0 + 10, (PY0 + PY1) / 2)
      return
    }
    if (!this.upperEdges) {
      const keep: number[] = []
      for (let i = 0; i < m.edges.length; i += 2) {
        const a = m.edges[i]
        const b = m.edges[i + 1]
        if (m.verts[a * 3 + 1] >= -0.02 && m.verts[b * 3 + 1] >= -0.02) {
          keep.push(a, b)
        }
      }
      this.upperEdges = Uint32Array.from(keep)
    }
    // rotacao lenta + respiro sutil com o rms
    const ry = t * 0.22
    const rx = -0.34 + Math.sin(t * 0.13) * 0.06
    const scale = 500 * (1 + this.rmsS * 0.25)
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
    // arestas em 4 baldes de profundidade (tras esmaece)
    const paths: Path2D[] = [new Path2D(), new Path2D(), new Path2D(), new Path2D()]
    const edges = this.upperEdges
    const ne = edges.length / 2
    for (let i = 0; i < ne; i++) {
      const a = edges[i * 2]
      const b = edges[i * 2 + 1]
      const z = (sz[a] + sz[b]) * 0.5
      const bucket = Math.min(3, Math.max(0, Math.floor((z + 0.55) * 3.6)))
      paths[bucket].moveTo(sx[a], sy[a])
      paths[bucket].lineTo(sx[b], sy[b])
    }
    g.save()
    g.beginPath()
    g.rect(p.x0 + 4, PY0 + 26, p.x1 - p.x0 - 8, PY1 - PY0 - 32)
    g.clip()
    g.lineWidth = 0.55
    const alphas = [0.05, 0.11, 0.2, 0.36]
    const bris = [120, 150, 185, 225]
    for (let k = 0; k < 4; k++) {
      g.strokeStyle = ph(bris[k], alphas[k])
      g.stroke(paths[k])
    }
    g.restore()

    g.font = '9px Lilex, monospace'
    g.fillStyle = ph(160)
    const deg = ((ry * 180 / Math.PI) % 360 + 360) % 360
    g.fillText(`RY ${deg.toFixed(1).padStart(5, '0')}°`, p.x1 - 70, PY0 + 16)
    g.fillStyle = ph(105)
    g.fillText(`VTX ${n} EDG ${ne}`, p.x1 - 118, 183)
    void f
  }

  private drawFmap(g: CanvasRenderingContext2D, f: AudioFrame): void {
    const p = P.fmap
    const x0 = p.x0 + 10
    const x1 = p.x1 - 96
    const y0 = 66
    const y1 = PY1 - 16
    const wpx = x1 - x0
    // acento sutil: a linha do espectro leva o gradiente (freq -> cor)
    g.strokeStyle = gradPaint(g, x0, x1)
    g.beginPath()
    const tops = new Float32Array(wpx + 1)
    for (let k = 0; k <= wpx; k++) {
      const v = f.spec[Math.floor((k / wpx) * (SPEC_N - 1))] / 255
      const y = y1 - v * (y1 - y0)
      tops[k] = y
      if (k === 0) g.moveTo(x0 + k, y)
      else g.lineTo(x0 + k, y)
    }
    g.stroke()
    // dither de preenchimento (tique do lumiere)
    g.fillStyle = ph(60)
    for (let k = 0; k < wpx; k += 4) {
      for (let yy = tops[k] + 4; yy < y1 - 2; yy += 6) {
        g.fillRect(x0 + k, yy, 1, 1)
      }
    }
    // deltas por banda
    g.font = '10px Lilex, monospace'
    const bands = [f.low, f.mid, f.high]
    const lbls = ['LOW', 'MID', 'HI ']
    for (let k = 0; k < 3; k++) {
      g.fillStyle = ph(160)
      g.fillText(lbls[k], p.x1 - 84, 80 + k * 18)
      g.fillStyle = ph(210)
      g.fillText(`Δ ${(bands[k] * 99).toFixed(1).padStart(4, '0')}`, p.x1 - 58, 80 + k * 18)
    }
    g.fillStyle = ph(130)
    g.fillText('...', p.x1 - 84, 80 + 3 * 18)
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
    this.rmsS += (f.rms - this.rmsS) * 0.18

    this.drawHeader(g, t)
    this.drawInput(g, f)
    this.drawWave(g, f)
    this.drawSgram(g, f)
    this.drawEntity(g, f, t, dt)
    this.drawMesh(g, f, t)
    this.drawFmap(g, f)
    this.frameFresh = false
  }

  /** painel de input em coords de cena (pro hit test do clique) */
  inputPanelHit(x: number, y: number): boolean {
    return x >= P.input.x0 && x <= P.input.x1 && y >= PY0 && y <= PY1
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
