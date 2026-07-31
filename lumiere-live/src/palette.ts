// gradiente da faixa: 6 pontas fixas do dono, espalhadas no eixo x.
// a cena e desenhada em fosforo cinza (hierarquia = brilho, como no
// lumiere) e recebe a matiz numa unica passada de blend 'color' por
// frame; quem precisa de cor pontual usa grad()/gradCss().

export const STOPS = [
  '#22DFEE', '#163BDD', '#7320D9', '#D920AA', '#F38F61', '#E4F361',
] as const

const RGB = STOPS.map((h) => [
  parseInt(h.slice(1, 3), 16),
  parseInt(h.slice(3, 5), 16),
  parseInt(h.slice(5, 7), 16),
]) as [number, number, number][]

/** cor do gradiente em u = 0..1 (interp linear entre as pontas) */
export function grad(u: number): [number, number, number] {
  const t = Math.min(Math.max(u, 0), 1) * (RGB.length - 1)
  const i = Math.min(Math.floor(t), RGB.length - 2)
  const f = t - i
  const a = RGB[i]
  const b = RGB[i + 1]
  return [
    a[0] + (b[0] - a[0]) * f,
    a[1] + (b[1] - a[1]) * f,
    a[2] + (b[2] - a[2]) * f,
  ]
}

export function gradCss(u: number, alpha = 1): string {
  const [r, g, b] = grad(u)
  return `rgba(${r | 0},${g | 0},${b | 0},${alpha})`
}

/** paint de gradiente horizontal cobrindo x0..x1 (coords de cena) */
export function gradPaint(
  g: CanvasRenderingContext2D, x0: number, x1: number,
): CanvasGradient {
  const p = g.createLinearGradient(x0, 0, x1, 0)
  STOPS.forEach((c, i) => p.addColorStop(i / (STOPS.length - 1), c))
  return p
}

/** gradiente pastel: cores puxadas pro branco de fosforo (suave,
    pra areas onde a cor crua fica dura demais, ex goniometro) */
export function gradPaintSoft(
  g: CanvasRenderingContext2D, x0: number, x1: number, mix = 0.45,
): CanvasGradient {
  const p = g.createLinearGradient(x0, 0, x1, 0)
  RGB.forEach((c, i) => {
    const r = (c[0] + (214 - c[0]) * mix) | 0
    const gg = (c[1] + (228 - c[1]) * mix) | 0
    const b = (c[2] + (238 - c[2]) * mix) | 0
    p.addColorStop(i / (RGB.length - 1), `rgb(${r},${gg},${b})`)
  })
  return p
}

/** fosforo frio do lumiere: brilho v (0..255) com o tint da lei
    visual (r = v*0.94, g = v*0.97, b = v*1.01 + 6) */
export function ph(v: number, alpha = 1): string {
  const c = Math.min(Math.max(v, 0), 255)
  const r = (c * 0.94) | 0
  const g = (c * 0.97) | 0
  const b = Math.min(255, c * 1.01 + 6) | 0
  return alpha >= 1 ? `rgb(${r},${g},${b})` : `rgba(${r},${g},${b},${alpha})`
}
