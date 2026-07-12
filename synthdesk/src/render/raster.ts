// raster de svg pro canvas com cache; quando a imagem carrega, o
// renderer registrado redesenha (frame sujo)
const cache = new Map<string, HTMLImageElement>()
let onReady: (() => void) | null = null

export function setRasterCallback(cb: () => void): void {
  onReady = cb
}

export function rasterSvg(key: string, raw: string): HTMLImageElement | null {
  let img = cache.get(key)
  if (!img) {
    img = new Image()
    img.src = `data:image/svg+xml;utf8,${encodeURIComponent(raw)}`
    img.onload = () => onReady?.()
    cache.set(key, img)
  }
  return img.complete && img.naturalWidth > 0 ? img : null
}
