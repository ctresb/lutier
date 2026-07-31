// lumiere-live: visualizador de sinal em tempo real pra rodape de
// stream. cena 2d nitida + glow webgl (plus-lighter) + overlays css.
import './style.css'
import { Scene, W, H } from './scene'
import { CrtFx } from './crt'
import { connect, AudioBridge } from './audio'
import { parseGlb } from './glb'
import meshUrl from './assets/meiaum.glb'

const sceneCanvas = document.getElementById('scene') as HTMLCanvasElement
const glowCanvas = document.getElementById('glow') as HTMLCanvasElement

async function boot(): Promise<void> {
  try { await document.fonts.load('12px Lilex') } catch { /* fallback mono */ }

  const dpr = Math.min(window.devicePixelRatio || 1, 2)
  const scene = new Scene(sceneCanvas, dpr)

  let crt: CrtFx | null = null
  try {
    crt = new CrtFx(glowCanvas)
  } catch {
    glowCanvas.remove()
  }

  // mesh 3d (wireframe do meiaum.glb)
  fetch(meshUrl)
    .then((r) => r.arrayBuffer())
    .then((b) => { scene.mesh = parseGlb(b) })
    .catch(() => { /* sem mesh o painel avisa LOADING */ })

  ;(window as unknown as { __scene: Scene }).__scene = scene

  const bridge: AudioBridge = await connect((f) => scene.setFrame(f))
  const syncDevice = (): void => {
    scene.deviceName = bridge.devices[bridge.current] ?? 'NO INPUT'
    scene.deviceIdx = bridge.current
    scene.deviceCount = bridge.devices.length
  }
  syncDevice()

  const toScene = (ev: MouseEvent): [number, number] => {
    const r = sceneCanvas.getBoundingClientRect()
    return [((ev.clientX - r.left) / r.width) * W, ((ev.clientY - r.top) / r.height) * H]
  }
  sceneCanvas.addEventListener('click', (ev) => {
    const [x, y] = toScene(ev)
    if (scene.inputPanelHit(x, y)) bridge.cycle(1).then(syncDevice)
  })
  sceneCanvas.addEventListener('contextmenu', (ev) => {
    ev.preventDefault()
    const [x, y] = toScene(ev)
    if (scene.inputPanelHit(x, y)) bridge.cycle(-1).then(syncDevice)
  })

  const loop = (now: number): void => {
    scene.render(now)
    crt?.present(sceneCanvas)
    requestAnimationFrame(loop)
  }
  requestAnimationFrame(loop)
}

boot()
