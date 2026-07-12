import { COL } from '../core/palette'

// cantos em bracket, assinatura visual do lumiere
export function brackets(
  g: CanvasRenderingContext2D,
  x0: number,
  y0: number,
  x1: number,
  y1: number,
  len = 10,
  color: string = COL.bracket,
): void {
  g.strokeStyle = color
  g.beginPath()
  for (const [cx, cy, dx, dy] of [
    [x0, y0, 1, 1],
    [x1, y0, -1, 1],
    [x0, y1, 1, -1],
    [x1, y1, -1, -1],
  ] as const) {
    g.moveTo(cx + len * dx, cy)
    g.lineTo(cx, cy)
    g.lineTo(cx, cy + len * dy)
  }
  g.stroke()
}

export function text(
  g: CanvasRenderingContext2D,
  s: string,
  x: number,
  y: number,
  size: number,
  color: string,
  align: CanvasTextAlign = 'left',
): void {
  g.font = `${size}px Lilex, monospace`
  g.textAlign = align
  g.textBaseline = 'top'
  g.fillStyle = color
  g.fillText(s, x, y)
}
