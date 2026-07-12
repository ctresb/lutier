import { spec } from '../components/registry'
import { HEADER_H, IO_H, portsOf, sizeOf } from '../components/spec'
import { nodePort, onOff, renderControl } from './controls'
import type { Camera } from '../core/camera'
import { Graph } from '../core/graph'
import { COL, ph, phHex } from '../core/palette'
import { settings } from '../core/settings'
import type { Cable, KnobSpec, NodeState, PortSpec, Vec2 } from '../core/types'
import { iconRaw, type IconName } from '../ui/icon'
import { brackets, text } from './prims'
import { rasterSvg, setRasterCallback } from './raster'

const GRID = 46 // passo da grade do lumiere

export interface DragCablePreview {
  from: { node: NodeState; port: PortSpec }
  toWorld: Vec2
  snap: { node: NodeState; port: PortSpec } | null
}

// renderer de passada unica com dirty flag: so desenha quando algo mudou
export class Renderer {
  private g: CanvasRenderingContext2D
  private dirty = true
  private dpr = 1

  hoverNode: NodeState | null = null
  hoverKnob: KnobSpec | null = null
  hoverSlider: string | null = null
  hoverPort: PortSpec | null = null
  hoverLock: number | null = null
  selectedNodes = new Set<number>()
  // cantos de cabo selecionados pela marquee, chave `${cableId}:${index}`
  selectedWaypoints = new Set<string>()
  selectedCable: number | null = null
  dragCable: DragCablePreview | null = null
  // caixa de multiselecao (cmd + arrasto), em coordenadas de mundo
  marquee: { x0: number; y0: number; x1: number; y1: number } | null = null
  private marqueeFade: { rect: { x0: number; y0: number; x1: number; y1: number }; start: number } | null = null

  // solta a caixa com fadeout curto (some na hora com reduced motion)
  endMarquee(): void {
    if (!this.marquee) return
    if (!matchMedia('(prefers-reduced-motion: reduce)').matches) {
      this.marqueeFade = { rect: this.marquee, start: performance.now() }
    }
    this.marquee = null
    this.invalidate()
  }

  selectOnly(id: number): void {
    this.selectedNodes.clear()
    this.selectedWaypoints.clear()
    this.selectedNodes.add(id)
  }

  clearSelection(): void {
    this.selectedNodes.clear()
    this.selectedWaypoints.clear()
  }

  // desenha na cena 2d (canvas offscreen quando ha pos-fx) e entrega
  // o frame pronto pro compositor via post; display e quem tem layout
  constructor(
    private display: HTMLCanvasElement,
    private scene: HTMLCanvasElement,
    private cam: Camera,
    private graph: Graph,
    private post: ((scene: HTMLCanvasElement) => void) | null = null,
  ) {
    const g = scene.getContext('2d', { alpha: false })
    if (!g) throw new Error('canvas 2d indisponivel')
    this.g = g
    setRasterCallback(() => this.invalidate())
    this.resize()
    new ResizeObserver(() => {
      this.resize()
      this.invalidate()
    }).observe(display)
    const loop = () => {
      if (this.dirty) {
        this.dirty = false
        this.draw()
      }
      requestAnimationFrame(loop)
    }
    requestAnimationFrame(loop)
  }

  invalidate(): void {
    this.dirty = true
  }

  private resize(): void {
    this.dpr = Math.min(window.devicePixelRatio || 1, 2)
    const w = this.display.clientWidth
    const h = this.display.clientHeight
    this.scene.width = Math.max(1, Math.round(w * this.dpr))
    this.scene.height = Math.max(1, Math.round(h * this.dpr))
    this.cam.vw = w
    this.cam.vh = h
  }

  private draw(): void {
    const { g, cam, dpr } = this
    g.setTransform(dpr, 0, 0, dpr, 0, 0)
    // vetores com juncoes e pontas arredondadas, sempre
    g.lineJoin = 'round'
    g.lineCap = 'round'
    g.fillStyle = COL.bg
    g.fillRect(0, 0, cam.vw, cam.vh)

    // espaco de mundo
    g.setTransform(
      cam.z * dpr,
      0,
      0,
      cam.z * dpr,
      (cam.vw / 2 - cam.x * cam.z) * dpr,
      (cam.vh / 2 - cam.y * cam.z) * dpr,
    )
    const hairline = 1 / cam.z
    g.lineWidth = hairline

    if (settings.snapGrid) this.drawGrid()
    this.drawOrigin()
    // cabos por cima dos componentes, sempre
    for (const n of this.graph.nodes) this.drawNode(n)
    for (const c of this.graph.cables) this.drawCable(c)
    if (this.dragCable) this.drawDragCable(this.dragCable)
    if (this.marquee) this.drawMarquee(this.marquee, 1)
    if (this.marqueeFade) {
      const k = 1 - (performance.now() - this.marqueeFade.start) / 180
      if (k <= 0) {
        this.marqueeFade = null
      } else {
        this.drawMarquee(this.marqueeFade.rect, k)
        this.dirty = true // continua animando ate sumir
      }
    }

    // componentes animados (ex: scope do oscillator ligado) pedem
    // o proximo frame; mesa sem animacao volta pro zero trabalho
    if (this.graph.nodes.some((n) => spec(n.type).animates?.(n, this.graph))) this.dirty = true

    this.post?.(this.scene)
  }

  // grade de pontos com LOD: passo dobra ate ficar legivel na tela,
  // pontos da camada grossa sempre firmes, camada fina esvaece no zoom out
  private drawGrid(): void {
    const { g, cam } = this
    let s = GRID
    while (s * cam.z < 23) s *= 2
    while (s * cam.z >= 46 && s > GRID) s /= 2
    const fade = Math.min(1, Math.max(0, (s * cam.z - 20) / 26))

    const w0 = cam.toWorld(0, 0)
    const w1 = cam.toWorld(cam.vw, cam.vh)
    const x0 = Math.floor(w0.x / s) * s
    const y0 = Math.floor(w0.y / s) * s
    const r = Math.max(0.6, 0.85 / cam.z)

    const fine = new Path2D()
    const coarse = new Path2D()
    for (let x = x0; x <= w1.x; x += s) {
      const ix = Math.round(x / s)
      for (let y = y0; y <= w1.y; y += s) {
        const iy = Math.round(y / s)
        const p = ix % 2 === 0 && iy % 2 === 0 ? coarse : fine
        p.rect(x - r / 2, y - r / 2, r, r)
      }
    }
    g.fillStyle = COL.dot
    g.fill(coarse)
    if (fade > 0.02) {
      g.fillStyle = ph(255, 0.115 * fade)
      g.fill(fine)
    }
  }

  private drawOrigin(): void {
    const { g, cam } = this
    const l = 14 / cam.z
    g.strokeStyle = COL.lineFaint
    g.beginPath()
    g.moveTo(-l, 0)
    g.lineTo(l, 0)
    g.moveTo(0, -l)
    g.lineTo(0, l)
    g.stroke()
    text(g, '/ORIGIN', l + 6 / cam.z, -5 / cam.z, Math.max(9, 10 / cam.z), COL.textFaint)
  }

  // base compartilhada de todo componente: fundo, moldura dupla,
  // brackets, header (nome/tag/lock) e a faixa inferior de io
  private drawNode(n: NodeState): void {
    const { g } = this
    const s = spec(n.type)
    const { w, h } = sizeOf(s)
    const sel = this.selectedNodes.has(n.id)
    const hov = this.hoverNode?.id === n.id

    // fundo 100% opaco: grade e cabos nao vazam pelo componente
    g.fillStyle = 'rgb(5 7 9)'
    g.fillRect(n.x, n.y, w, h)

    // moldura dupla do lumiere
    g.lineWidth = 1 / this.cam.z
    g.strokeStyle = sel ? COL.lineMid : COL.line
    g.strokeRect(n.x + 0.5, n.y + 0.5, w - 1, h - 1)
    g.strokeStyle = COL.lineFaint
    g.strokeRect(n.x + 3.5, n.y + 3.5, w - 7, h - 7)
    brackets(g, n.x, n.y, n.x + w, n.y + h, 10, sel || hov ? COL.textBright : COL.bracket)

    // header: energia (switch + ON/OFF) a esquerda, nome/tag ao
    // lado, toggle de lock a direita
    const powered = (n.params.on ?? 1) > 0
    g.save()
    g.translate(n.x, n.y)
    // sem label: o switch centra sozinho com o bloco nome+tag
    onOff(g, 12, 15, powered, false)
    g.restore()
    text(g, n.name, n.x + 48, n.y + 10, 12, sel ? COL.textBright : COL.text)
    text(g, s.tag, n.x + 48, n.y + 26, 9, COL.textFaint)
    const lockHover = this.hoverLock === n.id
    const lockTint = n.locked ? (lockHover ? 247 : 205) : lockHover ? 190 : 105
    const lockImg = this.iconImage(n.locked ? 'lock-closed' : 'lock-open', phHex(lockTint))
    if (lockImg) g.drawImage(lockImg, n.x + w - 26, n.y + 14, 14, 14)
    g.strokeStyle = COL.lineFaint
    g.beginPath()
    g.moveTo(n.x + 12, n.y + HEADER_H)
    g.lineTo(n.x + w - 12, n.y + HEADER_H)
    g.stroke()

    // miolo: controles declarados no json da base + desenho extra
    const opts = {
      zoom: this.cam.z,
      selected: sel,
      hoverKnob: hov ? (this.hoverKnob?.param ?? null) : null,
      hoverSlider: hov ? this.hoverSlider : null,
      hoverPort: hov ? this.hoverPort : null,
      cvInto: (port: string) => this.graph.cvInto(n.id, port),
    }
    g.save()
    g.translate(n.x, n.y)
    for (const c of s.controls) renderControl(g, s, n, c, opts)
    s.drawExtra?.(g, n, opts)
    g.restore()

    // faixa inferior padronizada: componente nodePort (label +
    // quadrado com folgas simetricas dentro da faixa)
    g.strokeStyle = COL.lineFaint
    g.beginPath()
    g.moveTo(n.x + 12, n.y + h - IO_H)
    g.lineTo(n.x + w - 12, n.y + h - IO_H)
    g.stroke()
    for (const p of portsOf(s)) {
      nodePort(g, n.x + p.x, n.y + h - IO_H, p.label, this.portActive(n, p))
    }
  }

  // icone do dono (svg de src/icons) rasterizado com tint de fosforo
  private iconImage(name: IconName, tint: string): HTMLImageElement | null {
    return rasterSvg(`icon/${name}/${tint}`, iconRaw(name).replace(/currentColor/g, tint))
  }


  private portActive(n: NodeState, p: PortSpec): boolean {
    return (
      (this.hoverNode?.id === n.id && this.hoverPort?.id === p.id) ||
      (this.dragCable?.snap?.node.id === n.id && this.dragCable.snap.port.id === p.id) ||
      this.graph.cables.some(
        (c) =>
          (c.from.node === n.id && c.from.port === p.id) ||
          (c.to.node === n.id && c.to.port === p.id),
      )
    )
  }

  // cabo reto: polilinha port -> waypoints -> port
  private drawCable(c: Cable): void {
    const { g } = this
    const pts = this.graph.cablePts(c)
    const sel = this.selectedCable === c.id
    g.strokeStyle = sel ? COL.textBright : ph(185)
    g.lineWidth = (sel ? 1.6 : 1.1) / this.cam.z
    g.beginPath()
    g.moveTo(pts[0].x, pts[0].y)
    for (let k = 1; k < pts.length; k++) g.lineTo(pts[k].x, pts[k].y)
    g.stroke()
    g.lineWidth = 1 / this.cam.z

    // pontas nos ports: quadradinhos (nada de bolinha)
    g.fillStyle = COL.textBright
    for (const q of [pts[0], pts[pts.length - 1]]) {
      g.fillRect(q.x - 2.5, q.y - 2.5, 5, 5)
    }
    // handles de roteamento: quadrados retos; canto selecionado
    // (cabo inteiro ou marquee) fica preenchido
    for (let k = 0; k < c.pts.length; k++) {
      const p = c.pts[k]
      if (sel || this.selectedWaypoints.has(`${c.id}:${k}`)) {
        g.fillStyle = COL.textBright
        g.fillRect(p.x - 3.5, p.y - 3.5, 7, 7)
      } else {
        g.strokeStyle = COL.lineMid
        g.strokeRect(p.x - 3.5, p.y - 3.5, 7, 7)
      }
    }
  }

  // outline + fill translucido bem sutil, mas visivel
  private drawMarquee(m: { x0: number; y0: number; x1: number; y1: number }, k: number): void {
    const { g } = this
    const x = Math.min(m.x0, m.x1)
    const y = Math.min(m.y0, m.y1)
    const w = Math.abs(m.x1 - m.x0)
    const h = Math.abs(m.y1 - m.y0)
    g.globalAlpha = k
    g.fillStyle = ph(255, 0.05)
    g.fillRect(x, y, w, h)
    g.strokeStyle = COL.lineMid
    g.strokeRect(x, y, w, h)
    g.globalAlpha = 1
  }

  private drawDragCable(d: DragCablePreview): void {
    const { g } = this
    const a = this.graph.portPos({ node: d.from.node.id, port: d.from.port.id })
    const b = d.snap
      ? this.graph.portPos({ node: d.snap.node.id, port: d.snap.port.id })
      : d.toWorld
    g.strokeStyle = d.snap ? COL.textBright : ph(150)
    g.lineWidth = 1.1 / this.cam.z
    g.beginPath()
    g.moveTo(a.x, a.y)
    g.lineTo(b.x, b.y)
    g.stroke()
    g.lineWidth = 1 / this.cam.z
  }
}
