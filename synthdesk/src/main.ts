import './style/tokens.css'
import './style/base.css'
import './style/hud.css'
import './style/componentbox.css'

import { deskAudio } from './audio/audio'
import { Camera } from './core/camera'
import { linkEngine } from './core/engine'
import { Graph } from './core/graph'
import { Input } from './core/input'
import { CrtFx } from './render/crt'
import { Renderer } from './render/renderer'
import { spec } from './components/registry'
import { sizeOf } from './components/spec'
import { ProjectStore } from './core/project'
import { getVar, listVars, setVar } from './core/vars'
import { initComponentBox } from './ui/componentbox'
import { initContextMenu } from './ui/contextmenu'
import { setNodeCount, setZoom } from './ui/hud'
import { initToolbox } from './ui/toolbox'

const display = document.getElementById('desk') as HTMLCanvasElement
const glowCanvas = document.getElementById('glow') as HTMLCanvasElement

// cena 2d nitida direto na tela; bloom e um canvas webgl2 pequeno
// somado por cima (plus-lighter). sem webgl2, fica so a cena.
let fx: CrtFx | null = null
try {
  fx = new CrtFx(glowCanvas)
} catch {
  glowCanvas.remove()
}

const cam = new Camera()
const graph = new Graph()
const renderer = new Renderer(display, display, cam, graph, (c) => {
  // frame sujo = momento certo de reconciliar o audio com o grafo
  deskAudio.sync(graph)
  fx?.present(c)
})
const input = new Input(display, cam, graph, renderer)

// variaveis globais da mesa (OSC_01_ACTIVE, DEV_01_NOTE...): a
// superficie publica de automacao - escrever aqui mexe no componente
// e o audio reconcilia no proximo frame
const vars = {
  list: () => listVars(graph),
  get: (name: string) => getVar(graph, name),
  set: (name: string, value: number | string | boolean): boolean => {
    const ok = setVar(graph, name, value)
    if (ok) renderer.invalidate() // frame sujo tambem re-sincroniza o audio
    return ok
  },
}
Object.assign(window, { desk: { vars } })

// projeto .synthproj: save/load + autosave (localStorage e, com
// arquivo aberto, no proprio arquivo); atalhos cmd+s / cmd+shift+s /
// cmd+o registrados no init
const project = new ProjectStore(graph, cam, () => {
  renderer.clearSelection()
  renderer.invalidate()
})

// fonte unica do projeto: redesenha quando a lilex carregar
document.fonts.ready.then(() => renderer.invalidate())

// handle de inspecao so no dev server
if (import.meta.env.DEV) {
  Object.assign(window, { __desk: { graph, renderer, cam, input, audio: deskAudio } })
}

initComponentBox(input)
initContextMenu(display, cam, graph, renderer, input)
initToolbox([
  {
    icon: 'save',
    name: 'SAVE PROJECT',
    run: () => void project.save(),
  },
  {
    icon: 'load',
    name: 'LOAD PROJECT',
    run: () => void project.load(),
  },
  {
    icon: 'centralize',
    name: 'CENTRALIZE',
    run: () => {
      // enquadra o patch inteiro na vista (mesa vazia volta pra origem)
      if (graph.nodes.length === 0) {
        cam.reset()
      } else {
        let x0 = Infinity
        let y0 = Infinity
        let x1 = -Infinity
        let y1 = -Infinity
        for (const n of graph.nodes) {
          const s = sizeOf(spec(n.type))
          x0 = Math.min(x0, n.x)
          y0 = Math.min(y0, n.y)
          x1 = Math.max(x1, n.x + s.w)
          y1 = Math.max(y1, n.y + s.h)
        }
        cam.x = (x0 + x1) / 2
        cam.y = (y0 + y1) / 2
        const fit = Math.min((cam.vw - 200) / (x1 - x0), (cam.vh - 200) / (y1 - y0))
        cam.z = Math.min(1.5, Math.max(0.15, fit))
      }
      setZoom(cam.z)
      renderer.invalidate()
    },
  },
])
setZoom(cam.z)
setNodeCount(0)
project.init() // recupera a sessao do autosave e liga os atalhos
void linkEngine()
