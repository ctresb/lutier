// modulo de camera: pega a webcam disponivel, extrai SO os contornos
// (sobel) numa grade pixelada e desenha linha fina com aberracao
// cromatica suave (3 passes rgb deslocados em 'lighter').
// a grade tem o MESMO aspect do painel de destino e o video entra
// por recorte cover: nada de esticar/distorcer.

const GH = 82
const GW_MAX = 480

export class CamFx {
  state: 'idle' | 'starting' | 'ok' | 'fail' = 'idle'
  private video = document.createElement('video')
  private gw = 0
  private grab = document.createElement('canvas')
  private grabCtx: CanvasRenderingContext2D
  private lum = new Float32Array(0)
  private edges = document.createElement('canvas')
  private edgesCtx: CanvasRenderingContext2D
  private edgesData: ImageData | null = null
  private tints: HTMLCanvasElement[] = []

  constructor() {
    this.grabCtx = this.grab.getContext('2d', { willReadFrequently: true })!
    this.edgesCtx = this.edges.getContext('2d')!
    for (let i = 0; i < 3; i++) this.tints.push(document.createElement('canvas'))
    this.video.muted = true
    this.video.playsInline = true
  }

  start(): void {
    if (this.state !== 'idle') return
    this.state = 'starting'
    navigator.mediaDevices.getUserMedia({
      video: { width: { ideal: 640 }, height: { ideal: 360 } },
      audio: false,
    }).then((stream) => {
      this.video.srcObject = stream
      return this.video.play()
    }).then(() => {
      this.state = 'ok'
    }).catch(() => {
      this.state = 'fail'
    })
  }

  /** (re)dimensiona a grade pro aspect do painel de destino */
  private ensure(aspect: number): void {
    const gw = Math.min(GW_MAX, Math.round(GH * aspect))
    if (gw === this.gw) return
    this.gw = gw
    this.grab.width = gw
    this.grab.height = GH
    this.edges.width = gw
    this.edges.height = GH
    this.edgesData = this.edgesCtx.createImageData(gw, GH)
    this.lum = new Float32Array(gw * GH)
    for (const t of this.tints) {
      t.width = gw
      t.height = GH
    }
  }

  private process(): void {
    const gw = this.gw
    const g = this.grabCtx
    // recorte cover: preenche a grade mantendo a proporcao do video
    const vw = this.video.videoWidth || 640
    const vh = this.video.videoHeight || 360
    const scale = Math.max(gw / vw, GH / vh)
    const sw = gw / scale
    const sh = GH / scale
    g.drawImage(this.video, (vw - sw) / 2, (vh - sh) / 2, sw, sh, 0, 0, gw, GH)
    const d = g.getImageData(0, 0, gw, GH).data
    const lum = this.lum
    for (let i = 0; i < gw * GH; i++) {
      lum[i] = d[i * 4] * 0.299 + d[i * 4 + 1] * 0.587 + d[i * 4 + 2] * 0.114
    }
    const out = this.edgesData!.data
    out.fill(0)
    for (let y = 1; y < GH - 1; y++) {
      for (let x = 1; x < gw - 1; x++) {
        const i = y * gw + x
        const gx = -lum[i - gw - 1] - 2 * lum[i - 1] - lum[i + gw - 1]
          + lum[i - gw + 1] + 2 * lum[i + 1] + lum[i + gw + 1]
        const gy = -lum[i - gw - 1] - 2 * lum[i - gw] - lum[i - gw + 1]
          + lum[i + gw - 1] + 2 * lum[i + gw] + lum[i + gw + 1]
        const mag = Math.abs(gx) + Math.abs(gy)
        if (mag > 130) {
          const o = i * 4
          const v = Math.min(255, 120 + mag * 0.35)
          out[o] = v
          out[o + 1] = v
          out[o + 2] = v
          out[o + 3] = 255
        }
      }
    }
    this.edgesCtx.putImageData(this.edgesData!, 0, 0)
    // tres tints pro rgb split
    const cols = ['#ff5f6d', '#7dffa8', '#6db2ff']
    for (let k = 0; k < 3; k++) {
      const tc = this.tints[k].getContext('2d')!
      tc.clearRect(0, 0, gw, GH)
      tc.drawImage(this.edges, 0, 0)
      tc.globalCompositeOperation = 'source-in'
      tc.fillStyle = cols[k]
      tc.fillRect(0, 0, gw, GH)
      tc.globalCompositeOperation = 'source-over'
    }
  }

  /** desenha os contornos pixelados no retangulo dado (sem distorcer:
      a grade nasce com o aspect do retangulo) */
  draw(g: CanvasRenderingContext2D, x: number, y: number,
    w: number, h: number): void {
    if (this.state !== 'ok' || this.video.readyState < 2) return
    this.ensure(w / h)
    this.process()
    const sm = g.imageSmoothingEnabled
    g.imageSmoothingEnabled = false
    g.globalCompositeOperation = 'lighter'
    const ab = Math.max(1.2, w / this.gw)
    g.globalAlpha = 0.62
    g.drawImage(this.tints[0], x - ab, y, w, h)
    g.drawImage(this.tints[2], x + ab, y, w, h)
    g.globalAlpha = 0.9
    g.drawImage(this.tints[1], x, y, w, h)
    g.globalAlpha = 1
    g.globalCompositeOperation = 'source-over'
    g.imageSmoothingEnabled = sm
  }
}
