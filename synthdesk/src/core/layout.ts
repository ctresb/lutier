import { spec } from '../components/registry'
import { portsOf, sizeOf, UNIT } from '../components/spec'
import type { Graph } from './graph'
import type { NodeState, Vec2 } from './types'
import { settings } from './settings'

interface Box {
  x: number
  y: number
  w: number
  h: number
}

function boxOf(n: NodeState): Box {
  const s = sizeOf(spec(n.type))
  return { x: n.x, y: n.y, w: s.w, h: s.h }
}

// bordas encostadas (gap zero) NAO contam como sobreposicao
function overlaps(a: Box, b: Box): boolean {
  return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
}

// snap magnetico de bordas estilo janelas do macos: perto de uma
// lateral (horizontal ou vertical) de outro componente, cola flush;
// longe, movimento livre. intencao = distancia menor que o limiar.
export function snapMove(
  node: NodeState,
  nx: number,
  ny: number,
  graph: Graph,
  zoom: number,
  exclude?: Set<number>,
): Vec2 {
  const s = sizeOf(spec(node.type))
  const thr = 12 / zoom
  let bx = nx
  let by = ny
  let dxBest = thr
  let dyBest = thr
  let snappedX = false
  let snappedY = false

  for (const m of graph.nodes) {
    if (m.id === node.id || exclude?.has(m.id)) continue
    const o = boxOf(m)
    const vNear = ny < o.y + o.h + thr && ny + s.h > o.y - thr
    const hNear = nx < o.x + o.w + thr && nx + s.w > o.x - thr

    if (vNear) {
      // colar na direita ou na esquerda do vizinho
      for (const cand of [o.x + o.w, o.x - s.w]) {
        const d = Math.abs(nx - cand)
        if (d < dxBest) {
          dxBest = d
          bx = cand
          snappedX = true
        }
      }
    }
    if (hNear) {
      // colar embaixo ou em cima do vizinho
      for (const cand of [o.y + o.h, o.y - s.h]) {
        const d = Math.abs(ny - cand)
        if (d < dyBest) {
          dyBest = d
          by = cand
          snappedY = true
        }
      }
    }
    // alinhamento secundario de bordas no eixo perpendicular
    if (snappedX) {
      for (const cand of [o.y, o.y + o.h - s.h]) {
        const d = Math.abs(ny - cand)
        if (d < dyBest) {
          dyBest = d
          by = cand
          snappedY = true
        }
      }
    }
    if (snappedY) {
      for (const cand of [o.x, o.x + o.w - s.w]) {
        const d = Math.abs(nx - cand)
        if (d < dxBest) {
          dxBest = d
          bx = cand
          snappedX = true
        }
      }
    }
  }

  if (!snappedX) bx = settings.snapGrid ? Math.round(bx / UNIT) * UNIT : Math.round(bx)
  if (!snappedY) by = settings.snapGrid ? Math.round(by / UNIT) * UNIT : Math.round(by)
  return { x: bx, y: by }
}

// nenhum componente pode ficar em cima de outro: se a posicao atual
// sobrepoe, procura o lugar vazio mais proximo (aneis crescentes de
// 1 unidade, menor distancia euclidiana ganha)
export function resolveCollision(node: NodeState, graph: Graph): void {
  const box = boxOf(node)
  const others = graph.nodes.filter((n) => n.id !== node.id).map(boxOf)
  const fits = (x: number, y: number): boolean =>
    others.every((o) => !overlaps({ x, y, w: box.w, h: box.h }, o))
  if (fits(node.x, node.y)) return

  for (let r = 1; r <= 80; r++) {
    let best: Vec2 | null = null
    let bestD = Infinity
    for (let dx = -r; dx <= r; dx++) {
      for (const dy of Math.abs(dx) === r ? range(-r, r) : [-r, r]) {
        const x = node.x + dx * UNIT
        const y = node.y + dy * UNIT
        if (!fits(x, y)) continue
        const d = dx * dx + dy * dy
        if (d < bestD) {
          bestD = d
          best = { x, y }
        }
      }
    }
    if (best) {
      node.x = best.x
      node.y = best.y
      return
    }
  }
}

function range(a: number, b: number): number[] {
  const out: number[] = []
  for (let i = a; i <= b; i++) out.push(i)
  return out
}

// largou um componente com in E out em cima de um cabo: emenda.
// A -> B vira A -> componente -> B (primeiro in e primeiro out).
export function trySplice(node: NodeState, graph: Graph): boolean {
  const s = spec(node.type)
  const ports = portsOf(s)
  const pin = ports.find((p) => p.dir === 'in')
  const pout = ports.find((p) => p.dir === 'out')
  if (!pin || !pout) return false
  const { w, h } = sizeOf(s)

  for (const c of [...graph.cables]) {
    if (c.from.node === node.id || c.to.node === node.id) continue
    const pts = graph.cablePts(c)
    for (let k = 0; k < pts.length - 1; k++) {
      if (!segIntersectsRect(pts[k], pts[k + 1], node.x, node.y, node.x + w, node.y + h)) continue
      const srcNode = graph.node(c.from.node)
      const dstNode = graph.node(c.to.node)
      if (!srcNode || !dstNode) return false
      const srcPort = portsOf(spec(srcNode.type)).find((p) => p.id === c.from.port)
      const dstPort = portsOf(spec(dstNode.type)).find((p) => p.id === c.to.port)
      if (!srcPort || !dstPort) return false
      graph.removeCable(c.id)
      graph.connect({ node: srcNode, port: srcPort }, { node, port: pin })
      graph.connect({ node, port: pout }, { node: dstNode, port: dstPort })
      return true
    }
  }
  return false
}

// clip liang-barsky: segmento cruza (ou entra em) um retangulo?
function segIntersectsRect(a: Vec2, b: Vec2, x0: number, y0: number, x1: number, y1: number): boolean {
  const dx = b.x - a.x
  const dy = b.y - a.y
  let t0 = 0
  let t1 = 1
  const p = [-dx, dx, -dy, dy]
  const q = [a.x - x0, x1 - a.x, a.y - y0, y1 - a.y]
  for (let i = 0; i < 4; i++) {
    if (p[i] === 0) {
      if (q[i] < 0) return false
    } else {
      const r = q[i] / p[i]
      if (p[i] < 0) {
        if (r > t1) return false
        if (r > t0) t0 = r
      } else {
        if (r < t0) return false
        if (r < t1) t1 = r
      }
    }
  }
  return true
}
