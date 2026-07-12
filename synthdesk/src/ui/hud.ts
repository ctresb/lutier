// atualiza os campos dinamicos das barras do HUD (DOM direto, sem framework)
const el = (id: string): HTMLElement => {
  const e = document.getElementById(id)
  if (!e) throw new Error(`hud sem elemento #${id}`)
  return e
}

export type Status =
  | 'IDLE'
  | 'PANNING'
  | 'PLACING'
  | 'PATCHING'
  | 'TUNING'
  | 'MOVING'
  | 'ROUTING'
  | 'SELECTING'

const status = el('hud-status')
const hx = el('hud-x')
const hy = el('hud-y')
const hz = el('hud-z')
const nodes = el('hud-nodes')
const engine = el('hud-engine')
const clock = el('hud-clock')
const snap = el('hud-snap')

export function setStatus(s: Status): void {
  status.textContent = s
}

export function setCoords(x: number, y: number): void {
  hx.textContent = (x >= 0 ? '+' : '-') + Math.abs(x).toFixed(1).padStart(6, '0')
  hy.textContent = (y >= 0 ? '+' : '-') + Math.abs(y).toFixed(1).padStart(6, '0')
}

export function setZoom(z: number): void {
  hz.textContent = `${Math.round(z * 100)}%`
}

export function setNodeCount(n: number): void {
  nodes.textContent = `NODES ${String(n).padStart(2, '0')}`
}

export function setEngine(label: string): void {
  engine.textContent = `LINK FEED: ${label}`
}

export function setSnap(on: boolean): void {
  snap.textContent = on ? 'SNAP ON' : 'SNAP OFF'
}

// relogio de sessao no header, estilo timecode
const t0 = performance.now()
setInterval(() => {
  const s = Math.floor((performance.now() - t0) / 1000)
  const hh = String(Math.floor(s / 3600)).padStart(2, '0')
  const mm = String(Math.floor((s % 3600) / 60)).padStart(2, '0')
  const ss = String(s % 60).padStart(2, '0')
  clock.textContent = `${hh}:${mm}:${ss}`
}, 1000)
