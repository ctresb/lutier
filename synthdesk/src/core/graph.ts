import { spec } from '../components/registry'
import { buttonsOf, knobsOf, portsOf, sizeOf, slidersOf, UNIT } from '../components/spec'
import type { Cable, Hit, NodeState, PortRef, PortSpec, Vec2 } from './types'

// grafo do patch: nodes + cabos, com hit testing em coordenadas de mundo
export class Graph {
  nodes: NodeState[] = []
  cables: Cable[] = []
  private nextNode = 1
  private nextCable = 1

  addNode(type: string, x: number, y: number): NodeState {
    const s = spec(type)
    const count = this.nodes.filter((n) => n.type === type).length + 1
    // 'on' e da base: todo componente nasce ligado
    const params: Record<string, number> = { on: 1 }
    for (const p of s.params) params[p.id] = p.def
    const node: NodeState = {
      id: this.nextNode++,
      type,
      name: `${s.prefix}_${String(count).padStart(2, '0')}`,
      x: Math.round(x),
      y: Math.round(y),
      params,
    }
    this.nodes.push(node)
    return node
  }

  // duplica um conjunto de componentes deslocado de 1u, preservando
  // params e os cabos INTERNOS ao conjunto (com waypoints)
  duplicate(ids: number[]): NodeState[] {
    const map = new Map<number, NodeState>()
    for (const id of ids) {
      const n = this.node(id)
      if (!n) continue
      const copy = this.addNode(n.type, n.x + UNIT, n.y + UNIT)
      copy.params = { ...n.params }
      map.set(id, copy)
    }
    for (const c of [...this.cables]) {
      const f = map.get(c.from.node)
      const t = map.get(c.to.node)
      if (f && t) {
        this.cables.push({
          id: this.nextCable++,
          from: { node: f.id, port: c.from.port },
          to: { node: t.id, port: c.to.port },
          pts: c.pts.map((p) => ({ x: p.x + UNIT, y: p.y + UNIT })),
        })
      }
    }
    return [...map.values()]
  }

  removeNode(id: number): void {
    this.nodes = this.nodes.filter((n) => n.id !== id)
    this.cables = this.cables.filter((c) => c.from.node !== id && c.to.node !== id)
  }

  removeCable(id: number): void {
    this.cables = this.cables.filter((c) => c.id !== id)
  }

  node(id: number): NodeState | undefined {
    return this.nodes.find((n) => n.id === id)
  }

  portPos(ref: PortRef): Vec2 {
    const n = this.node(ref.node)
    if (!n) return { x: 0, y: 0 }
    const p = portsOf(spec(n.type)).find((q) => q.id === ref.port)
    if (!p) return { x: n.x, y: n.y }
    return { x: n.x + p.x, y: n.y + p.y }
  }

  // valor de cv chegando num input (fonte conectada, se houver);
  // seen evita loop infinito em ciclos de cabos
  cvInto(nodeId: number, port: string, seen: Set<number> = new Set()): number | null {
    const c = this.cables.find((k) => k.to.node === nodeId && k.to.port === port)
    if (!c || seen.has(c.id)) return null
    seen.add(c.id)
    const src = this.node(c.from.node)
    // fonte desligada e inerte: como se nada estivesse plugado
    if (!src || (src.params.on ?? 1) <= 0) return null
    return spec(src.type).cvOut?.(src, c.from.port, this, seen) ?? null
  }

  // conecta out -> in; um in aceita um cabo so (o novo substitui).
  // como numa mesa analogica, tudo e tensao: qualquer out pluga em
  // qualquer in (cv em audio, audio em cv), sem restricao de tipo
  connect(a: { node: NodeState; port: PortSpec }, b: { node: NodeState; port: PortSpec }): Cable | null {
    const out = a.port.dir === 'out' ? a : b.port.dir === 'out' ? b : null
    const inn = a.port.dir === 'in' ? a : b.port.dir === 'in' ? b : null
    if (!out || !inn) return null
    if (out.node.id === inn.node.id) return null
    this.cables = this.cables.filter(
      (c) => !(c.to.node === inn.node.id && c.to.port === inn.port.id),
    )
    const cable: Cable = {
      id: this.nextCable++,
      from: { node: out.node.id, port: out.port.id },
      to: { node: inn.node.id, port: inn.port.id },
      pts: [],
    }
    this.cables.push(cable)
    return cable
  }

  // vertices do cabo em mundo: port de saida -> waypoints -> port de entrada
  cablePts(c: Cable): Vec2[] {
    return [this.portPos(c.from), ...c.pts, this.portPos(c.to)]
  }

  raiseNode(id: number): void {
    const i = this.nodes.findIndex((n) => n.id === id)
    if (i >= 0) this.nodes.push(this.nodes.splice(i, 1)[0])
  }

  // prioridade acompanha o empilhamento visual: ports (alvo pequeno,
  // cabos terminam neles) > waypoints > cabos (desenhados POR CIMA
  // dos componentes) > knob/botao/corpo do componente
  hitTest(wx: number, wy: number, zoom: number): Hit | null {
    const pr = Math.max(9, 11 / zoom)
    for (let i = this.nodes.length - 1; i >= 0; i--) {
      const n = this.nodes[i]
      for (const p of portsOf(spec(n.type))) {
        const dx = wx - (n.x + p.x)
        const dy = wy - (n.y + p.y)
        if (dx * dx + dy * dy <= pr * pr) return { t: 'port', node: n, port: p }
      }
    }
    const wr = Math.max(6, 8 / zoom)
    for (let i = this.cables.length - 1; i >= 0; i--) {
      const c = this.cables[i]
      for (let k = 0; k < c.pts.length; k++) {
        const dx = wx - c.pts[k].x
        const dy = wy - c.pts[k].y
        if (dx * dx + dy * dy <= wr * wr) return { t: 'waypoint', cable: c, index: k }
      }
    }
    const cable = this.hitCable(wx, wy, Math.max(6, 8 / zoom))
    if (cable) return cable
    for (let i = this.nodes.length - 1; i >= 0; i--) {
      const n = this.nodes[i]
      const s = spec(n.type)
      const { w, h } = sizeOf(s)
      if (wx >= n.x && wx <= n.x + w && wy >= n.y && wy <= n.y + h) {
        // toggle de lock no canto direito do header
        if (wx >= n.x + w - 32 && wy <= n.y + 34 && wy >= n.y + 6) {
          return { t: 'lock', node: n }
        }
        // switch de energia no canto esquerdo do header
        if (wx <= n.x + 40 && wy <= n.y + 34 && wy >= n.y + 6) {
          return { t: 'power', node: n }
        }
        for (const k of knobsOf(s)) {
          const dx = wx - (n.x + k.cx)
          const dy = wy - (n.y + k.cy)
          if (dx * dx + dy * dy <= (k.r + 6) * (k.r + 6)) return { t: 'knob', node: n, knob: k }
        }
        for (const sl of slidersOf(s)) {
          if (
            wx >= n.x + sl.x - 4 &&
            wx <= n.x + sl.x + sl.w + 4 &&
            wy >= n.y + sl.y - 8 &&
            wy <= n.y + sl.y + 8
          ) {
            return { t: 'slider', node: n, slider: sl }
          }
        }
        for (const b of buttonsOf(s)) {
          if (wx >= n.x + b.x && wx <= n.x + b.x + b.w && wy >= n.y + b.y && wy <= n.y + b.y + b.h) {
            return { t: 'button', node: n, button: b.id }
          }
        }
        return { t: 'body', node: n }
      }
    }
    return null
  }

  private hitCable(wx: number, wy: number, tol: number): Hit | null {
    for (let i = this.cables.length - 1; i >= 0; i--) {
      const c = this.cables[i]
      const pts = this.cablePts(c)
      for (let k = 0; k < pts.length - 1; k++) {
        const q = closestOnSeg(wx, wy, pts[k], pts[k + 1])
        if (Math.hypot(wx - q.x, wy - q.y) <= tol) return { t: 'cable', cable: c, seg: k, at: q }
      }
    }
    return null
  }
}

function closestOnSeg(px: number, py: number, a: Vec2, b: Vec2): Vec2 {
  const dx = b.x - a.x
  const dy = b.y - a.y
  const l2 = dx * dx + dy * dy
  const t = l2 === 0 ? 0 : Math.max(0, Math.min(1, ((px - a.x) * dx + (py - a.y) * dy) / l2))
  return { x: a.x + t * dx, y: a.y + t * dy }
}
