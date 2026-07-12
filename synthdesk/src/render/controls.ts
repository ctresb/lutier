import type { ComponentSpec, ControlSpec, DrawOpts } from '../components/spec'
import { sizeOf } from '../components/spec'
import { COL } from '../core/palette'
import type { NodeState } from '../core/types'
import { text } from './prims'

// controles padrao do synthdesk, todos desenhados pela base a partir
// da declaracao json do componente. nada de layout proprio.

// arco de 270 graus comecando as 7h30 (135deg), como pot analogico
const A0 = (Math.PI * 3) / 4
const SWEEP = (Math.PI * 3) / 2

export function knob(
  g: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  r: number,
  v: number,
  hot: boolean,
): void {
  // ticks do curso, a cada 15 graus
  g.strokeStyle = COL.lineFaint
  g.beginPath()
  for (let i = 0; i <= 18; i++) {
    const a = A0 + (SWEEP * i) / 18
    const r0 = r + 5
    const r1 = r + (i % 3 === 0 ? 11 : 8)
    g.moveTo(cx + Math.cos(a) * r0, cy + Math.sin(a) * r0)
    g.lineTo(cx + Math.cos(a) * r1, cy + Math.sin(a) * r1)
  }
  g.stroke()

  // trilho apagado + trilho percorrido
  g.strokeStyle = COL.lineFaint
  g.lineWidth = 1
  g.beginPath()
  g.arc(cx, cy, r, A0, A0 + SWEEP)
  g.stroke()
  g.strokeStyle = hot ? COL.textBright : COL.lineMid
  g.beginPath()
  g.arc(cx, cy, r, A0, A0 + SWEEP * v)
  g.stroke()

  // corpo do knob
  g.strokeStyle = hot ? COL.bracket : COL.line
  g.beginPath()
  g.arc(cx, cy, r - 6, 0, Math.PI * 2)
  g.stroke()

  // ponteiro (o halo vem do bloom global, nada de shadowBlur)
  const a = A0 + SWEEP * v
  g.save()
  g.strokeStyle = COL.textBright
  g.lineWidth = 1.6
  g.beginPath()
  g.moveTo(cx + Math.cos(a) * 5, cy + Math.sin(a) * 5)
  g.lineTo(cx + Math.cos(a) * (r - 8), cy + Math.sin(a) * (r - 8))
  g.stroke()
  g.restore()
  g.fillStyle = COL.textDim
  g.beginPath()
  g.arc(cx, cy, 2, 0, Math.PI * 2)
  g.fill()
}

// slider horizontal padrao: label faint + valor a direita na linha de
// cima; trilho apagado full-width em y+18 (respiro: o handle nao
// invade o texto), trecho percorrido lineMid (bright em hover),
// handle quadrado 7x7 preenchido (linguagem do waypoint)
export function slider(
  g: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  v: number,
  label: string,
  value: string,
  hot: boolean,
): void {
  text(g, label, x, y, 9, COL.textFaint)
  text(g, value, x + w, y, 9, COL.text, 'right')
  const ty = y + 18
  g.strokeStyle = COL.lineFaint
  g.beginPath()
  g.moveTo(x, ty)
  g.lineTo(x + w, ty)
  g.stroke()
  g.strokeStyle = hot ? COL.textBright : COL.lineMid
  g.beginPath()
  g.moveTo(x, ty)
  g.lineTo(x + v * w, ty)
  g.stroke()
  g.fillStyle = COL.textBright
  g.fillRect(x + v * w - 3.5, ty - 3.5, 7, 7)
}

// toggle padrao: quadrado 10x10 com quadradinho 4x4 (mesmo visual do
// port ativo). label: false = sem texto.
export function toggle(
  g: CanvasRenderingContext2D,
  x: number,
  y: number,
  on: boolean,
  label: string | false,
): void {
  g.strokeStyle = on ? COL.textBright : COL.lineMid
  g.strokeRect(x, y, 10, 10)
  if (on) {
    g.fillStyle = COL.textBright
    g.fillRect(x + 3, y + 3, 4, 4)
  }
  if (label !== false) {
    // -1 centra o texto renderizado da lilex 9px com o quadrado de 10
    // (validado no olho contra o render real, nao so no TextMetrics)
    text(g, label, x + 17, y - 1, 9, on ? COL.text : COL.textDim)
  }
}

// switch padrao: retangulo 24x12 com cursor 8x8 que desliza;
// esquerda = off (escuro), direita = on (claro)
export function switchCtl(
  g: CanvasRenderingContext2D,
  x: number,
  y: number,
  on: boolean,
  label: string | false,
): void {
  g.strokeStyle = on ? COL.textBright : COL.lineMid
  g.strokeRect(x, y, 24, 12)
  if (on) {
    g.fillStyle = COL.textBright
    g.fillRect(x + 14, y + 2, 8, 8)
  } else {
    g.fillStyle = COL.textFaint
    g.fillRect(x + 2, y + 2, 8, 8)
  }
  if (label !== false) {
    text(g, label, x + 31, y, 9, on ? COL.text : COL.textDim)
  }
}

// componente ON/OFF: switch com a label de estado EMBAIXO (diz ON ou
// OFF sozinha, conforme o estado); label opcional
export function onOff(
  g: CanvasRenderingContext2D,
  x: number,
  y: number,
  on: boolean,
  label: boolean,
): void {
  switchCtl(g, x, y, on, false)
  if (label) {
    text(g, on ? 'ON' : 'OFF', x + 12, y + 16, 8, on ? COL.text : COL.textFaint, 'center')
  }
}

// port de componente na faixa de io: label 8px + quadrado 10x10
// empilhados, folgas iguais medidas contra o FRAME INTERNO do
// componente (linha +5, label, vao 4, quadrado em +16..+26)
export function nodePort(
  g: CanvasRenderingContext2D,
  x: number,
  lineY: number,
  label: string,
  active: boolean,
): void {
  text(g, label, x, lineY + 5, 8, COL.textFaint, 'center')
  const cy = lineY + 21
  g.strokeStyle = active ? COL.textBright : COL.lineMid
  g.strokeRect(x - 5, cy - 5, 10, 10)
  if (active) {
    g.fillStyle = COL.textBright
    g.fillRect(x - 2, cy - 2, 4, 4)
  }
}

// renderiza um controle declarado no json da base
export function renderControl(
  g: CanvasRenderingContext2D,
  spec: ComponentSpec,
  node: NodeState,
  c: ControlSpec,
  o: DrawOpts,
): void {
  const { w } = sizeOf(spec)
  switch (c.kind) {
    case 'knob':
      knob(g, c.x, c.y, c.r, node.params[c.param] ?? 0, o.hoverKnob === c.param)
      break
    case 'slider': {
      const v = node.params[c.param] ?? 0
      const disp = spec.sliderValue?.(node, c.param) ?? v.toFixed(3)
      slider(g, c.x, c.y, c.w, v, c.label, disp, o.hoverSlider === c.param)
      break
    }
    case 'toggle':
      toggle(g, c.x, c.y, (node.params[c.param] ?? 0) > 0, c.label)
      break
    case 'switch':
      switchCtl(g, c.x, c.y, (node.params[c.param] ?? 0) > 0, c.label)
      break
    case 'selector': {
      text(g, c.label, c.x, c.y, 9, COL.textFaint)
      const value = spec.selectorValue?.(node, c.id) ?? ''
      text(g, value, w - 14, c.y, 9, COL.text, 'right')
      break
    }
    case 'readout':
      text(g, (node.params[c.param] ?? 0).toFixed(3), c.x, c.y, 13, COL.textBright, 'center')
      break
    case 'label':
      text(g, c.text, c.x, c.y, 9, COL.textFaint, 'center')
      break
    case 'rule':
      g.strokeStyle = COL.lineFaint
      g.beginPath()
      g.moveTo(12, c.y)
      g.lineTo(w - 12, c.y)
      g.stroke()
      break
  }
}
