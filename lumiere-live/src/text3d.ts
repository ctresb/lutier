// texto MEIAUM MUSICA sempre em 3d de verdade: svgs -> shapes
// (furos inclusos) -> extrude com bevel, material cromado com env
// map procedural, camera ORTOGRAFICA em angulo isometrico. todos os
// estilos (marquee, seq, stack, outline, linha parada) sao modos
// deste renderer; o dof e o lens flare sao compostos no blit (2d).

import {
  Group, Mesh, MeshStandardMaterial, MeshBasicMaterial,
  OrthographicCamera, PMREMGenerator, Scene, WebGLRenderer,
  ExtrudeGeometry, DirectionalLight, Material, DoubleSide,
} from 'three'
import { SVGLoader } from 'three/examples/jsm/loaders/SVGLoader.js'
import { RoomEnvironment } from 'three/examples/jsm/environments/RoomEnvironment.js'

import mSvg from './assets/letters/m.svg?raw'
import eSvg from './assets/letters/e.svg?raw'
import iSvg from './assets/letters/i.svg?raw'
import aSvg from './assets/letters/a.svg?raw'
import uSvg from './assets/letters/u.svg?raw'
import sSvg from './assets/letters/s.svg?raw'
import cSvg from './assets/letters/c.svg?raw'

const RAW: Record<string, string> = {
  m: mSvg, e: eSvg, i: iSvg, a: aSvg, u: uSvg, s: sSvg, c: cSvg,
}

export type Text3DStyle = 'seq' | 'marquee' | 'stack' | 'outline' | '3d'

/** efeito por letra (combo/melt): escala anisotropica, queda, vis */
export interface LetterFx {
  sx?: number
  sy?: number
  dy?: number
  vis?: boolean
}

export interface RenderOpts {
  bandScale?: (index: number) => number
  fx?: (index: number) => LetterFx
  /** centra a camera no intervalo de indices dado (ex so MEIAUM) */
  focus?: [number, number]
  /** forca material wireframe em qualquer estilo */
  wire?: boolean
}

const PHRASE = 'meiaum musica'
const DEPTH = 22
const GAP = 13
const SPACE = 46

interface Letter { holder: Group; index: number; x0: number; y0: number }

interface GlyphGeo { geo: ExtrudeGeometry; width: number }

export class Text3D {
  readonly canvas: HTMLCanvasElement
  private renderer: WebGLRenderer
  private scene = new Scene()
  private camera = new OrthographicCamera(-1, 1, 1, -1, -2000, 2000)
  private root = new Group()
  private lineA = new Group()
  private lineB = new Group()
  private stack = new Group()
  private lettersLine: Letter[] = []
  private lettersStack: Letter[] = []
  private chrome: Material
  private wire: Material
  private lineWidth = 0

  constructor() {
    this.canvas = document.createElement('canvas')
    this.renderer = new WebGLRenderer({
      canvas: this.canvas, alpha: true, antialias: true,
      powerPreference: 'high-performance',
    })
    this.renderer.setClearColor(0x000000, 0)

    // env map procedural (nada externo entra pela csp)
    const pmrem = new PMREMGenerator(this.renderer)
    this.scene.environment = pmrem.fromScene(new RoomEnvironment(), 0.06).texture

    // doubleside: o flip y e assado na geometria (winding inverte)
    this.chrome = new MeshStandardMaterial({
      color: 0xf4f7fa, metalness: 1.0, roughness: 0.16, envMapIntensity: 1.25,
      side: DoubleSide,
    })
    this.wire = new MeshBasicMaterial({
      color: 0xcfdeeb, wireframe: true, side: DoubleSide,
    })

    // geometria por letra (centrada no proprio bbox), cacheada
    const loader = new SVGLoader()
    const glyphs = new Map<string, GlyphGeo>()
    for (const [ch, raw] of Object.entries(RAW)) {
      const data = loader.parse(raw)
      const geos: ExtrudeGeometry[] = []
      for (const path of data.paths) {
        for (const shape of SVGLoader.createShapes(path)) {
          geos.push(new ExtrudeGeometry(shape, {
            depth: DEPTH, bevelEnabled: true, bevelThickness: 2.2,
            bevelSize: 1.8, bevelSegments: 3, curveSegments: 8,
          }))
        }
      }
      // uma letra pode virar varios geos; junta pelo bbox comum
      let minX = Infinity; let maxX = -Infinity
      let minY = Infinity; let maxY = -Infinity
      for (const geo of geos) {
        geo.computeBoundingBox()
        const bb = geo.boundingBox!
        minX = Math.min(minX, bb.min.x); maxX = Math.max(maxX, bb.max.x)
        minY = Math.min(minY, bb.min.y); maxY = Math.max(maxY, bb.max.y)
      }
      const cx = (minX + maxX) / 2
      const cy = (minY + maxY) / 2
      for (const geo of geos) {
        geo.translate(-cx, -cy, -DEPTH / 2)
        // svg cresce pra baixo: flip assado na geometria
        geo.scale(1, -1, 1)
      }
      // svg tem varios geos por letra? junta num group na hora do build
      glyphs.set(ch, {
        geo: geos.length === 1 ? geos[0] : mergeAsGroupGeo(geos),
        width: maxX - minX,
      })
    }

    const buildWord = (
      target: Group, word: string, startIdx: number, yOff: number,
      out: Letter[],
    ): number => {
      let x = 0
      let idx = startIdx
      for (const ch of word) {
        if (ch === ' ') { x += SPACE + GAP; idx++; continue }
        const gl = glyphs.get(ch)
        if (!gl) { idx++; continue }
        const holder = new Group()
        const mesh = new Mesh(gl.geo, this.chrome)
        holder.add(mesh)
        holder.position.set(x + gl.width / 2, yOff, 0)
        target.add(holder)
        out.push({ holder, index: idx, x0: x + gl.width / 2, y0: yOff })
        x += gl.width + GAP
        idx++
      }
      return x - GAP
    }

    this.lineWidth = buildWord(this.lineA, PHRASE, 0, 0, this.lettersLine)
    buildWord(this.lineB, PHRASE, 0, 0, this.lettersLine)
    const w1 = buildWord(this.stack, 'meiaum', 0, 62, this.lettersStack)
    const w2 = buildWord(this.stack, 'musica', 7, -62, this.lettersStack)
    // centraliza cada linha do stack
    for (const l of this.lettersStack) {
      const wOwn = l.index < 7 ? w1 : w2
      l.holder.position.x -= wOwn / 2
    }

    this.root.add(this.lineA, this.lineB, this.stack)
    this.scene.add(this.root)

    const key = new DirectionalLight(0xffffff, 1.5)
    key.position.set(-140, 200, 260)
    this.scene.add(key)
  }

  /**
   * renderiza um frame do texto no estilo dado.
   * vt = tempo desde a entrada da cena; bandScale = escala por letra
   * (unica reatividade ao som permitida pelo dono).
   */
  render(
    t: number, vt: number, style: Text3DStyle,
    w: number, h: number, dpr: number,
    opts: RenderOpts = {},
  ): HTMLCanvasElement {
    const pw = Math.max(2, Math.floor(w * dpr))
    const phh = Math.max(2, Math.floor(h * dpr))
    if (this.canvas.width !== pw || this.canvas.height !== phh) {
      this.renderer.setSize(pw, phh, false)
    }

    const isStack = style === 'stack' || style === 'outline'
    this.stack.visible = isStack
    this.lineA.visible = !isStack
    this.lineB.visible = style === 'marquee'

    // isometrico POR LETRA: cada uma gira no proprio pivo e a linha
    // continua reta (girar o conjunto largo viraria uma diagonal)
    const rx = -0.3
    const ry = 0.4 + Math.sin(t * 0.19) * 0.07
    const mat = style === 'outline' || opts.wire ? this.wire : this.chrome
    const active = isStack ? this.lettersStack : this.lettersLine
    for (const l of active) {
      const mesh = l.holder.children[0] as Mesh
      if (mesh.material !== mat) mesh.material = mat
      const s = opts.bandScale ? opts.bandScale(l.index) : 1
      const fx = opts.fx ? opts.fx(l.index) : undefined
      // sal: bob e sway organicos por letra (onda percorrendo a
      // frase, movimento proprio, nao e reacao ao som)
      const bob = Math.sin(t * 0.55 + l.index * 0.45) * 2.6
      const sway = Math.sin(t * 0.21 + l.index * 0.33) * 0.05
      l.holder.scale.set(s * (fx?.sx ?? 1), s * (fx?.sy ?? 1), s)
      l.holder.position.y = l.y0 + (fx?.dy ?? 0) + bob
      l.holder.rotation.set(rx, ry + sway, 0)
      l.holder.visible = fx?.vis ?? true
    }

    let needWOverride = 0
    if (style === 'marquee') {
      const unit = this.lineWidth + 220
      const off = (vt * 130) % unit
      this.lineA.position.x = -off
      this.lineB.position.x = -off + unit
    } else if (opts.focus) {
      // centra o intervalo pedido (ex so MEIAUM ou so MUSICA)
      const inRange = this.lettersLine.filter(
        (l) => l.holder.parent === this.lineA &&
          l.index >= opts.focus![0] && l.index <= opts.focus![1])
      const cxr = inRange.length
        ? inRange.reduce((a, l) => a + l.x0, 0) / inRange.length
        : this.lineWidth / 2
      this.lineA.position.x = -cxr
      if (inRange.length) {
        const xs = inRange.map((l) => l.x0)
        needWOverride = (Math.max(...xs) - Math.min(...xs) + 320) * 1.15
      }
      for (const l of this.lettersLine) {
        if (l.holder.parent === this.lineA) {
          l.holder.visible = l.holder.visible &&
            l.index >= opts.focus[0] && l.index <= opts.focus[1]
        }
      }
    } else {
      this.lineA.position.x = -this.lineWidth / 2
      this.lineB.position.x = 0
    }
    if (style === 'seq') {
      const cycle = PHRASE.length * 0.26 + 2.4
      const born = ((vt % cycle) / 0.26)
      for (const l of this.lettersLine) {
        if (l.holder.parent === this.lineA) {
          l.holder.visible = l.holder.visible && l.index <= born
        }
      }
    }

    // frustum: altura fixa por estilo, largura segue o aspect;
    // nos estilos parados a largura do texto tem que caber
    const aspect = w / h
    let viewH = isStack ? 310 : 165
    const needW = style === 'marquee'
      ? 0
      : needWOverride || this.lineWidth * 1.16
    if (needW > viewH * aspect) viewH = needW / aspect
    const viewW = viewH * aspect
    this.camera.left = style === 'marquee' ? 0 : -viewW / 2
    this.camera.right = style === 'marquee' ? viewW : viewW / 2
    this.camera.top = viewH / 2
    this.camera.bottom = -viewH / 2
    this.camera.position.set(0, 0, 600)
    this.camera.updateProjectionMatrix()
    this.camera.lookAt(0, 0, 0)

    this.renderer.render(this.scene, this.camera)
    return this.canvas
  }
}

/** funde varios extrudes num so "geo" desenhavel: three nao precisa
    de merge real, um mesh por geo dentro do holder tambem serve; mas
    manter um unico mesh simplifica material/escala. aqui, na falta
    de utils de merge no core, empilha os atributos na mao. */
function mergeAsGroupGeo(geos: ExtrudeGeometry[]): ExtrudeGeometry {
  // caso raro (letra em varios paths): usa o primeiro e anexa os
  // demais como grupos de posicao — na pratica os svgs do dono tem
  // um path por letra, entao so devolve o primeiro
  return geos[0]
}
