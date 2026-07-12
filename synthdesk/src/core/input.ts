import { spec } from '../components/registry'
import { buttonsOf, sizeOf, UNIT } from '../components/spec'
import type { Renderer } from '../render/renderer'
import { setCoords, setNodeCount, setSnap, setStatus, setZoom } from '../ui/hud'
import type { Camera } from './camera'
import type { Graph } from './graph'
import { confirmDelete } from '../ui/confirm'
import { KnobGesture } from './knob'
import { resolveCollision, snapMove, trySplice } from './layout'
import { settings } from './settings'
import type { KnobSpec, NodeState, PortSpec, SliderSpec } from './types'

import type { Cable } from './types'

type Mode =
  | { m: 'idle' }
  | { m: 'pan' }
  | {
      m: 'move'
      // agarrado: componente OU canto de cabo selecionado
      grab: NodeState | null
      grabWp: { cable: Cable; index: number } | null
      ox: number
      oy: number
      // grupo selecionado se move junto, preservando as posicoes
      // relativas (componentes E cantos de cabo da marquee)
      group: { node: NodeState; sx: number; sy: number }[]
      wpts: { cable: Cable; index: number; sx: number; sy: number }[]
    }
  | { m: 'tune'; node: NodeState; knob: KnobSpec; gesture: KnobGesture }
  | { m: 'slide'; node: NodeState; slider: SliderSpec }
  | { m: 'patch'; from: { node: NodeState; port: PortSpec } }
  | { m: 'route'; cable: Cable; index: number; inserted: boolean; sx: number; sy: number }
  | { m: 'marquee' }

// toda a interacao do desk: pan, zoom no cursor, arrastar modulo,
// girar knob, puxar cabo, selecao e teclado
export class Input {
  private mode: Mode = { m: 'idle' }
  private lastX = 0
  private lastY = 0

  constructor(
    private canvas: HTMLCanvasElement,
    private cam: Camera,
    private graph: Graph,
    private r: Renderer,
  ) {
    canvas.addEventListener('pointerdown', (e) => this.down(e))
    canvas.addEventListener('pointermove', (e) => this.move(e))
    canvas.addEventListener('pointerup', (e) => this.up(e))
    canvas.addEventListener('pointercancel', () => this.reset())
    canvas.addEventListener('dblclick', (e) => this.dbl(e))
    canvas.addEventListener('wheel', (e) => this.wheel(e), { passive: false })
    window.addEventListener('keydown', (e) => this.key(e))
  }

  placeAt(type: string, sx: number, sy: number): void {
    const { w: cw, h: ch } = sizeOf(spec(type))
    const w = this.cam.toWorld(sx, sy)
    let x = w.x - cw / 2
    let y = w.y - ch / 2
    if (settings.snapGrid) {
      x = Math.round(x / UNIT) * UNIT
      y = Math.round(y / UNIT) * UNIT
    }
    const node = this.graph.addNode(type, x, y)
    resolveCollision(node, this.graph)
    trySplice(node, this.graph) // caiu em cima de um cabo? emenda
    this.afterGraphChange()
  }

  toggleSnap(): void {
    settings.snapGrid = !settings.snapGrid
    setSnap(settings.snapGrid)
    this.r.invalidate() // os pontinhos da grade somem junto
  }

  placeAtCenter(type: string): void {
    this.placeAt(type, this.cam.vw / 2, this.cam.vh / 2)
  }

  private afterGraphChange(): void {
    setNodeCount(this.graph.nodes.length)
    this.r.invalidate()
  }

  private applySlide(node: NodeState, sl: SliderSpec, wx: number): void {
    let v = Math.min(1, Math.max(0, (wx - (node.x + sl.x)) / sl.w))
    // slider com detents: cola na posicao mais proxima
    if (sl.steps && sl.steps > 1) v = Math.round(v * (sl.steps - 1)) / (sl.steps - 1)
    node.params[sl.param] = v
  }

  // apaga os cantos selecionados (indices decrescentes por cabo, pra
  // splice nao deslocar os proximos)
  private removeSelectedWpts(): void {
    const byCable = new Map<Cable, number[]>()
    for (const q of this.selectedWpts()) {
      const list = byCable.get(q.cable) ?? []
      list.push(q.index)
      byCable.set(q.cable, list)
    }
    for (const [c, idxs] of byCable) {
      idxs.sort((a, b) => b - a)
      for (const i of idxs) c.pts.splice(i, 1)
    }
    this.r.selectedWaypoints.clear()
  }

  // cantos de cabo selecionados, com posicao inicial pro arrasto
  private selectedWpts(): { cable: Cable; index: number; sx: number; sy: number }[] {
    const out: { cable: Cable; index: number; sx: number; sy: number }[] = []
    for (const c of this.graph.cables) {
      for (let k = 0; k < c.pts.length; k++) {
        if (this.r.selectedWaypoints.has(`${c.id}:${k}`)) {
          out.push({ cable: c, index: k, sx: c.pts[k].x, sy: c.pts[k].y })
        }
      }
    }
    return out
  }

  private down(e: PointerEvent): void {
    if (e.button !== 0 && e.button !== 1) return
    try {
      this.canvas.setPointerCapture(e.pointerId)
    } catch {
      // pointer ja encerrado (ex: evento sintetico); segue sem capture
    }
    this.lastX = e.clientX
    this.lastY = e.clientY
    const w = this.cam.toWorld(e.clientX, e.clientY)
    const hit = e.button === 1 ? null : this.graph.hitTest(w.x, w.y, this.cam.z)

    const multi = e.metaKey || e.ctrlKey
    this.r.selectedCable = null

    if ((!hit || e.button === 1) && multi && e.button === 0) {
      // cmd + arrasto no vazio: caixa de multiselecao
      this.r.clearSelection()
      this.mode = { m: 'marquee' }
      this.r.marquee = { x0: w.x, y0: w.y, x1: w.x, y1: w.y }
      setStatus('SELECTING')
      this.canvas.style.cursor = 'crosshair'
    } else if (!hit || e.button === 1) {
      this.r.clearSelection()
      this.mode = { m: 'pan' }
      setStatus('PANNING')
      this.canvas.style.cursor = 'grabbing'
    } else if (hit.t === 'knob') {
      this.mode = {
        m: 'tune',
        node: hit.node,
        knob: hit.knob,
        gesture: new KnobGesture(
          hit.node.params[hit.knob.param] ?? 0,
          e.clientY,
          spec(hit.node.type).knobMap?.(hit.knob.param) ?? null,
        ),
      }
      this.r.selectOnly(hit.node.id)
      setStatus('TUNING')
    } else if (hit.t === 'slider') {
      // posicionamento absoluto: o valor vai pra onde o cursor esta
      this.mode = { m: 'slide', node: hit.node, slider: hit.slider }
      this.applySlide(hit.node, hit.slider, w.x)
      this.r.selectOnly(hit.node.id)
      setStatus('TUNING')
    } else if (hit.t === 'lock') {
      // toggle direto no header, sem precisar do menu de contexto
      hit.node.locked = !hit.node.locked
      this.r.selectOnly(hit.node.id)
      this.mode = { m: 'idle' }
    } else if (hit.t === 'power') {
      // switch de energia da base: liga/desliga o componente
      hit.node.params.on = (hit.node.params.on ?? 1) > 0 ? 0 : 1
      this.r.selectOnly(hit.node.id)
      this.mode = { m: 'idle' }
    } else if (hit.t === 'button') {
      // controles bool (toggle/switch) a base resolve sozinha;
      // selector cai no hook press do componente
      const s = spec(hit.node.type)
      const zone = buttonsOf(s).find((b) => b.id === hit.button)
      if (zone && (zone.ctrl === 'toggle' || zone.ctrl === 'switch') && zone.param) {
        hit.node.params[zone.param] = hit.node.params[zone.param] ? 0 : 1
      } else {
        s.press?.(hit.node, hit.button)
      }
      this.r.selectOnly(hit.node.id)
      this.mode = { m: 'idle' }
    } else if (hit.t === 'port') {
      this.mode = { m: 'patch', from: { node: hit.node, port: hit.port } }
      this.r.dragCable = { from: this.mode.from, toWorld: w, snap: null }
      setStatus('PATCHING')
    } else if (hit.t === 'body') {
      if (multi) {
        // cmd + clique: alterna o componente na selecao
        if (this.r.selectedNodes.has(hit.node.id)) this.r.selectedNodes.delete(hit.node.id)
        else this.r.selectedNodes.add(hit.node.id)
        this.mode = { m: 'idle' }
      } else {
        this.graph.raiseNode(hit.node.id)
        if (!this.r.selectedNodes.has(hit.node.id)) this.r.selectOnly(hit.node.id)
        if (hit.node.locked) {
          // travado: seleciona mas nao move
          this.mode = { m: 'idle' }
        } else {
          const group = this.graph.nodes
            .filter((n) => this.r.selectedNodes.has(n.id) && !n.locked)
            .map((n) => ({ node: n, sx: n.x, sy: n.y }))
          this.mode = {
            m: 'move',
            grab: hit.node,
            grabWp: null,
            ox: w.x - hit.node.x,
            oy: w.y - hit.node.y,
            group,
            wpts: this.selectedWpts(), // cantos de cabo selecionados acompanham
          }
          setStatus('MOVING')
          this.canvas.style.cursor = 'grabbing'
        }
      }
    } else if (hit.t === 'waypoint') {
      const key = `${hit.cable.id}:${hit.index}`
      if (multi) {
        // cmd + clique: alterna o canto na selecao (igual componente)
        if (this.r.selectedWaypoints.has(key)) this.r.selectedWaypoints.delete(key)
        else this.r.selectedWaypoints.add(key)
        this.mode = { m: 'idle' }
      } else if (
        this.r.selectedWaypoints.has(key) &&
        this.r.selectedNodes.size + this.r.selectedWaypoints.size > 1
      ) {
        // canto selecionado num grupo: arrasta a selecao inteira junto
        const group = this.graph.nodes
          .filter((n) => this.r.selectedNodes.has(n.id) && !n.locked)
          .map((n) => ({ node: n, sx: n.x, sy: n.y }))
        const p = hit.cable.pts[hit.index]
        this.mode = {
          m: 'move',
          grab: null,
          grabWp: { cable: hit.cable, index: hit.index },
          ox: w.x - p.x,
          oy: w.y - p.y,
          group,
          wpts: this.selectedWpts(),
        }
        setStatus('MOVING')
        this.canvas.style.cursor = 'grabbing'
      } else {
        // arrasta ponto de roteamento sozinho (com snap ortogonal)
        this.r.selectedWaypoints.clear()
        this.mode = { m: 'route', cable: hit.cable, index: hit.index, inserted: false, sx: e.clientX, sy: e.clientY }
        this.r.selectedCable = hit.cable.id
        setStatus('ROUTING')
      }
    } else {
      // clique na linha: insere ponto de roteamento e ja arrasta;
      // se soltar sem mover, o ponto some e vira so selecao
      this.r.selectedWaypoints.clear() // indices do cabo mudam com o insert
      hit.cable.pts.splice(hit.seg, 0, { x: hit.at.x, y: hit.at.y })
      this.mode = { m: 'route', cable: hit.cable, index: hit.seg, inserted: true, sx: e.clientX, sy: e.clientY }
      this.r.selectedCable = hit.cable.id
      setStatus('ROUTING')
    }
    this.r.invalidate()
  }

  private move(e: PointerEvent): void {
    const w = this.cam.toWorld(e.clientX, e.clientY)
    setCoords(w.x, w.y)
    const dx = e.clientX - this.lastX
    const dy = e.clientY - this.lastY
    this.lastX = e.clientX
    this.lastY = e.clientY

    switch (this.mode.m) {
      case 'pan':
        this.cam.panScreen(dx, dy)
        this.r.invalidate()
        break
      case 'move': {
        // snap magnetico de bordas > snap de grade > livre (inteiro);
        // o agarrado dita o delta, o resto do grupo acompanha
        const mode = this.mode
        let dx = 0
        let dy = 0
        if (mode.grab) {
          const exclude = new Set(mode.group.map((g) => g.node.id))
          const p = snapMove(mode.grab, w.x - mode.ox, w.y - mode.oy, this.graph, this.cam.z, exclude)
          const grabStart = mode.group.find((g) => g.node.id === mode.grab!.id)
          dx = p.x - (grabStart?.sx ?? mode.grab.x)
          dy = p.y - (grabStart?.sy ?? mode.grab.y)
        } else if (mode.grabWp) {
          // agarrado num canto de cabo: delta livre (inteiro)
          const start = mode.wpts.find(
            (q) => q.cable.id === mode.grabWp!.cable.id && q.index === mode.grabWp!.index,
          )
          if (start) {
            dx = Math.round(w.x - mode.ox) - start.sx
            dy = Math.round(w.y - mode.oy) - start.sy
          }
        }
        for (const g of mode.group) {
          g.node.x = g.sx + dx
          g.node.y = g.sy + dy
        }
        for (const q of mode.wpts) {
          const p = q.cable.pts[q.index]
          if (p) {
            p.x = q.sx + dx
            p.y = q.sy + dy
          }
        }
        this.r.invalidate()
        break
      }
      case 'marquee': {
        if (this.r.marquee) {
          this.r.marquee.x1 = w.x
          this.r.marquee.y1 = w.y
          // selecao viva: tudo que intersecta a caixa
          const x0 = Math.min(this.r.marquee.x0, w.x)
          const y0 = Math.min(this.r.marquee.y0, w.y)
          const x1 = Math.max(this.r.marquee.x0, w.x)
          const y1 = Math.max(this.r.marquee.y0, w.y)
          this.r.selectedNodes.clear()
          for (const n of this.graph.nodes) {
            if (n.locked) continue // caixa de selecao ignora travados
            const s = sizeOf(spec(n.type))
            if (n.x < x1 && n.x + s.w > x0 && n.y < y1 && n.y + s.h > y0) {
              this.r.selectedNodes.add(n.id)
            }
          }
          // cantos de cabo dentro da caixa entram na selecao tambem
          this.r.selectedWaypoints.clear()
          for (const c of this.graph.cables) {
            for (let k = 0; k < c.pts.length; k++) {
              const p = c.pts[k]
              if (p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1) {
                this.r.selectedWaypoints.add(`${c.id}:${k}`)
              }
            }
          }
          this.r.invalidate()
        }
        break
      }
      case 'tune': {
        // primitivo de knob: incremental, shift so muda a precisao
        this.mode.node.params[this.mode.knob.param] = this.mode.gesture.move(
          e.clientY,
          e.shiftKey,
        )
        this.r.invalidate()
        break
      }
      case 'slide':
        this.applySlide(this.mode.node, this.mode.slider, w.x)
        this.r.invalidate()
        break
      case 'route': {
        // waypoint faz snap ORTOGONAL: perto de alinhar com um
        // vizinho da polilinha, cola no eixo dele (segmento vira
        // uma reta horizontal/vertical certinha). sem snap de grade.
        const mode = this.mode
        const p = mode.cable.pts[mode.index]
        if (p) {
          let x = Math.round(w.x)
          let y = Math.round(w.y)
          const verts = this.graph.cablePts(mode.cable)
          const prev = verts[mode.index]
          const next = verts[mode.index + 2]
          const thr = 8 / this.cam.z
          let bx = thr
          let by = thr
          for (const nb of [prev, next]) {
            if (!nb) continue
            const dx = Math.abs(x - nb.x)
            const dy = Math.abs(y - nb.y)
            if (dx < bx) {
              bx = dx
              x = nb.x
            }
            if (dy < by) {
              by = dy
              y = nb.y
            }
          }
          p.x = x
          p.y = y
          this.r.invalidate()
        }
        break
      }
      case 'patch': {
        const hit = this.graph.hitTest(w.x, w.y, this.cam.z)
        const from = this.mode.from
        // tudo e tensao: qualquer out casa com qualquer in (sem
        // restricao de kind, igual ao connect do grafo)
        const snap =
          hit && hit.t === 'port' && hit.node.id !== from.node.id && hit.port.dir !== from.port.dir
            ? { node: hit.node, port: hit.port }
            : null
        this.r.dragCable = { from, toWorld: w, snap }
        this.r.invalidate()
        break
      }
      case 'idle':
        this.hover(w.x, w.y)
        break
    }
  }

  private up(e: PointerEvent): void {
    if (this.mode.m === 'marquee') {
      this.r.endMarquee() // solta com fadeout, selecao fica
    }
    if (this.mode.m === 'patch' && this.r.dragCable) {
      const snap = this.r.dragCable.snap
      if (snap) this.graph.connect(this.r.dragCable.from, snap)
    } else if (this.mode.m === 'move') {
      // largou em cima de outro: cada um vai pro vazio mais proximo
      if (this.mode.grab) resolveCollision(this.mode.grab, this.graph)
      for (const g of this.mode.group) {
        if (g.node.id !== this.mode.grab?.id) resolveCollision(g.node, this.graph)
      }
      // largou UM componente em cima de um cabo: emenda na conexao
      if (this.mode.grab && this.mode.group.length === 1 && this.mode.wpts.length === 0) {
        trySplice(this.mode.grab, this.graph)
      }
    } else if (this.mode.m === 'route' && this.mode.inserted) {
      // clique seco na linha: nao deixa ponto orfao pra tras
      const moved = Math.hypot(e.clientX - this.mode.sx, e.clientY - this.mode.sy) > 4
      if (!moved) this.mode.cable.pts.splice(this.mode.index, 1)
    }
    const keepCable = this.mode.m === 'route' ? this.mode.cable.id : null
    this.reset()
    if (keepCable !== null) this.r.selectedCable = keepCable
    const w = this.cam.toWorld(e.clientX, e.clientY)
    this.hover(w.x, w.y)
  }

  private reset(): void {
    this.mode = { m: 'idle' }
    this.r.dragCable = null
    this.r.marquee = null
    setStatus('IDLE')
    setNodeCount(this.graph.nodes.length)
    this.canvas.style.cursor = 'default'
    this.r.invalidate()
  }

  private dbl(e: PointerEvent | MouseEvent): void {
    const w = this.cam.toWorld(e.clientX, e.clientY)
    const hit = this.graph.hitTest(w.x, w.y, this.cam.z)
    if (hit?.t === 'knob' || hit?.t === 'slider') {
      const param = hit.t === 'knob' ? hit.knob.param : hit.slider.param
      const def = spec(hit.node.type).params.find((p) => p.id === param)?.def ?? 0.5
      hit.node.params[param] = def
      this.r.invalidate()
    } else if (hit?.t === 'waypoint') {
      // duplo clique remove o ponto de roteamento; a selecao de
      // cantos esvazia (indices do cabo mudaram)
      hit.cable.pts.splice(hit.index, 1)
      this.r.selectedWaypoints.clear()
      this.r.invalidate()
    }
  }

  private wheel(e: WheelEvent): void {
    e.preventDefault()
    if (e.ctrlKey || e.metaKey) {
      // pinch do trackpad chega como wheel+ctrl
      this.cam.zoomAt(e.clientX, e.clientY, Math.exp(-e.deltaY * 0.011))
      setZoom(this.cam.z)
    } else {
      this.cam.panScreen(-e.deltaX, -e.deltaY)
    }
    const w = this.cam.toWorld(e.clientX, e.clientY)
    setCoords(w.x, w.y)
    this.r.invalidate()
  }

  private key(e: KeyboardEvent): void {
    const mod = e.metaKey || e.ctrlKey
    if (mod && e.key === '0') {
      this.cam.reset()
      setZoom(this.cam.z)
      this.r.invalidate()
      e.preventDefault()
    } else if (mod && (e.key === '=' || e.key === '+')) {
      this.cam.zoomAt(this.cam.vw / 2, this.cam.vh / 2, 1.2)
      setZoom(this.cam.z)
      this.r.invalidate()
      e.preventDefault()
    } else if (mod && e.key === '-') {
      this.cam.zoomAt(this.cam.vw / 2, this.cam.vh / 2, 1 / 1.2)
      setZoom(this.cam.z)
      this.r.invalidate()
      e.preventDefault()
    } else if (e.key === 'g' && !mod) {
      this.toggleSnap()
    } else if (e.key.toLowerCase() === 'd' && e.shiftKey && !mod) {
      // shift+d duplica a selecao (com cabos internos)
      const ids = [...this.r.selectedNodes]
      if (ids.length > 0) {
        const copies = this.graph.duplicate(ids)
        this.r.selectedNodes = new Set(copies.map((c) => c.id))
        for (const c of copies) resolveCollision(c, this.graph)
        this.afterGraphChange()
      }
      e.preventDefault()
    } else if (e.key === 'Escape') {
      this.reset()
    } else if (e.key === 'Delete' || e.key === 'Backspace') {
      const ids = [...this.r.selectedNodes].filter((id) => !this.graph.node(id)?.locked)
      if (ids.length > 0) {
        // confirmacao antes de apagar componentes
        void confirmDelete(ids.length).then((ok) => {
          if (!ok) return
          for (const id of ids) {
            this.graph.removeNode(id)
            this.r.selectedNodes.delete(id)
          }
          this.removeSelectedWpts() // cantos selecionados junto vao tambem
          this.afterGraphChange()
        })
      } else if (this.r.selectedWaypoints.size > 0) {
        // so cantos de cabo: apaga sem confirmacao (barato de refazer)
        this.removeSelectedWpts()
        this.r.invalidate()
      } else if (this.r.selectedCable !== null) {
        this.graph.removeCable(this.r.selectedCable)
        this.r.selectedCable = null
        this.r.invalidate()
      }
    }
  }

  private hover(wx: number, wy: number): void {
    const hit = this.graph.hitTest(wx, wy, this.cam.z)
    const prev = `${this.r.hoverNode?.id}/${this.r.hoverKnob?.param}/${this.r.hoverSlider}/${this.r.hoverPort?.id}/${this.r.hoverLock}`
    this.r.hoverNode =
      hit && hit.t !== 'cable' && hit.t !== 'waypoint' ? hit.node : null
    this.r.hoverKnob = hit?.t === 'knob' ? hit.knob : null
    this.r.hoverSlider = hit?.t === 'slider' ? hit.slider.param : null
    this.r.hoverPort = hit?.t === 'port' ? hit.port : null
    this.r.hoverLock = hit?.t === 'lock' ? hit.node.id : null
    this.canvas.style.cursor = !hit
      ? 'default'
      : hit.t === 'knob'
        ? 'ns-resize'
        : hit.t === 'slider'
          ? 'ew-resize'
          : hit.t === 'port'
            ? 'crosshair'
            : hit.t === 'body'
              ? 'grab'
              : 'pointer'
    const now = `${this.r.hoverNode?.id}/${this.r.hoverKnob?.param}/${this.r.hoverSlider}/${this.r.hoverPort?.id}/${this.r.hoverLock}`
    if (prev !== now) this.r.invalidate()
  }
}
