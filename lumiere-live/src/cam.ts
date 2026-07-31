// modulo de camera: pega a webcam disponivel, extrai SO os contornos
// (sobel) numa grade pixelada bem grossa e desenha linha fina com
// aberracao cromatica suave (3 passes rgb deslocados em 'lighter').

const GW = 152
const GH = 60

export class CamFx {
  state: 'idle' | 'starting' | 'ok' | 'fail' = 'idle'
  private video = document.createElement('video')
  private grab = document.createElement('canvas')
  private grabCtx: CanvasRenderingContext2D
  private lum = new Float32Array(GW * GH)
  private edges = document.createElement('canvas')
  private edgesCtx: CanvasRenderingContext2D
  private edgesData: ImageData
  private tints: HTMLCanvasElement[] = []

  constructor() {
    this.grab.width = GW
    this.grab.height = GH
    this.grabCtx = this.grab.getContext('2d', { willReadFrequently: true })!
    this.edges.width = GW
    this.edges.height = GH
    this.edgesCtx = this.edges.getContext('2d')!
    this.edgesData = this.edgesCtx.createImageData(GW, GH)
    for (let i = 0; i < 3; i++) {
      const c = document.createElement('canvas')
      c.width = GW
      c.height = GH
      this.tints.push(c)
    }
    this.video.muted = true
    this.video.playsInline = true
  }

  start(): void {
    if (this.state !== 'idle') return
    this.state = 'starting'
    navigator.mediaDevices.getUserMedia({
      video: { width: { ideal: 320 }, height: { ideal: 180 } },
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

  private process(): void {
    const g = this.grabCtx
    // crop cover: preenche a grade mantendo proporcao do video
    const vw = this.video.videoWidth || 320
    const vh = this.video.videoHeight || 180
    const scale = Math.max(GW / vw, GH / vh)
    const sw = GW / scale
    const sh = GH / scale
    g.drawImage(this.video, (vw - sw) / 2, (vh - sh) / 2, sw, sh, 0, 0, GW, GH)
    const d = g.getImageData(0, 0, GW, GH).data
    const lum = this.lum
    for (let i = 0; i < GW * GH; i++) {
      lum[i] = d[i * 4] * 0.299 + d[i * 4 + 1] * 0.587 + d[i * 4 + 2] * 0.114
    }
    const out = this.edgesData.data
    out.fill(0)
    for (let y = 1; y < GH - 1; y++) {
      for (let x = 1; x < GW - 1; x++) {
        const i = y * GW + x
        const gx = -lum[i - GW - 1] - 2 * lum[i - 1] - lum[i + GW - 1]
          + lum[i - GW + 1] + 2 * lum[i + 1] + lum[i + GW + 1]
        const gy = -lum[i - GW - 1] - 2 * lum[i - GW] - lum[i - GW + 1]
          + lum[i + GW - 1] + 2 * lum[i + GW] + lum[i + GW + 1]
        const mag = Math.abs(gx) + Math.abs(gy)
        if (mag > 96) {
          const o = i * 4
          const v = Math.min(255, 120 + mag * 0.35)
          out[o] = v
          out[o + 1] = v
          out[o + 2] = v
          out[o + 3] = 255
        }
      }
    }
    this.edgesCtx.putImageData(this.edgesData, 0, 0)
    // tres tints pro rgb split
    const cols = ['#ff5f6d', '#7dffa8', '#6db2ff']
    for (let k = 0; k < 3; k++) {
      const tc = this.tints[k].getContext('2d')!
      tc.clearRect(0, 0, GW, GH)
      tc.drawImage(this.edges, 0, 0)
      tc.globalCompositeOperation = 'source-in'
      tc.fillStyle = cols[k]
      tc.fillRect(0, 0, GW, GH)
      tc.globalCompositeOperation = 'source-over'
    }
  }

  /** desenha os contornos pixelados no retangulo dado */
  draw(g: CanvasRenderingContext2D, x: number, y: number,
    w: number, h: number): void {
    if (this.state !== 'ok' || this.video.readyState < 2) return
    this.process()
    const sm = g.imageSmoothingEnabled
    g.imageSmoothingEnabled = false
    g.globalCompositeOperation = 'lighter'
    const ab = Math.max(1.4, w / GW)
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
