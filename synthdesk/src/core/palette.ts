// mesma escala do lumiere (cinzas 0..255 com tint frio r*.94 g*.97 b*1.01),
// exposta como helper: ph(brilho, alpha) -> cor de fosforo pro canvas
// mesma cor em hex (pra tint de svg rasterizado no canvas)
export function phHex(v: number): string {
  const c = (n: number): string => Math.min(255, Math.round(n)).toString(16).padStart(2, '0')
  return `#${c(v * 0.94)}${c(v * 0.97)}${c(v * 1.01 + 6)}`
}

export function ph(v: number, a = 1): string {
  const r = Math.round(v * 0.94)
  const g = Math.round(v * 0.97)
  const b = Math.min(255, Math.round(v * 1.01 + 6))
  return a >= 1 ? `rgb(${r} ${g} ${b})` : `rgb(${r} ${g} ${b} / ${a})`
}

export const COL = {
  bg: '#030405',
  dot: ph(255, 0.115), // grade de pontos (26/255 do lumiere)
  dotHi: ph(255, 0.2),
  line: ph(110), // moldura
  lineFaint: ph(48),
  lineMid: ph(160),
  bracket: ph(228),
  textFaint: ph(105),
  textDim: ph(160),
  text: ph(205),
  textBright: ph(247),
} as const
