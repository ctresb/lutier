// componente de icone: TODOS os svgs sao feitos a mao pelo dono em
// src/icons/ (20x20, stroke/fill currentColor) pra manter a estetica.
// nada de biblioteca de icones no projeto.
const files = import.meta.glob('../icons/*.svg', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>

export type IconName =
  | 'lock-closed'
  | 'lock-open'
  | 'trash'
  | 'grid-enable'
  | 'grid-disable'
  | 'centralize'
  | 'save'
  | 'load'

export function icon(name: IconName): HTMLElement {
  const span = document.createElement('span')
  span.className = 'icon'
  span.setAttribute('aria-hidden', 'true')
  const raw = files[`../icons/${name}.svg`]
  if (raw) span.innerHTML = raw // svg estatico do proprio repo
  return span
}

// svg cru pra rasterizar no canvas (tint via currentColor)
export function iconRaw(name: IconName): string {
  return files[`../icons/${name}.svg`] ?? ''
}
