// bloom CRT barato: a cena 2d e mostrada direto (nitida, zero copia)
// e SO o glow e calculado num canvas webgl2 de 1/4 de resolucao,
// composto por cima com mix-blend-mode: plus-lighter (soma = sharp +
// glow, o mesmo compositor do lumiere). aberracao cromatica de ~2px
// so no glow. scanlines/vinheta/grain ficam nos overlays css.
// custo por frame sujo: um drawImage de downscale + upload pequeno
// (~1/16 dos pixels) + fragment em 1/4 de res. nada de mipmap.

const VERT = `#version 300 es
out vec2 vUv;
void main() {
  vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  vUv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`

const FRAG = `#version 300 es
precision mediump float;
uniform sampler2D uScene;
uniform vec2 uTexel;
uniform float uGain;
uniform float uBoost;
in vec2 vUv;
out vec4 o;

// media de 9 taps em anel: com a textura ja em 1/4 de res e filtro
// bilinear, r pequeno = halo de fosforo, r grande = halo de vidro
vec3 ring(vec2 uv, float r) {
  vec2 t = uTexel * r;
  vec3 s = texture(uScene, uv).rgb * 2.0;
  s += texture(uScene, uv + vec2(t.x, 0.0)).rgb;
  s += texture(uScene, uv - vec2(t.x, 0.0)).rgb;
  s += texture(uScene, uv + vec2(0.0, t.y)).rgb;
  s += texture(uScene, uv - vec2(0.0, t.y)).rgb;
  vec2 d = t * 0.7071;
  s += texture(uScene, uv + d).rgb;
  s += texture(uScene, uv - d).rgb;
  s += texture(uScene, uv + vec2(d.x, -d.y)).rgb;
  s += texture(uScene, uv - vec2(d.x, -d.y)).rgb;
  return s * 0.1;
}

vec3 halo(vec2 uv) {
  return ring(uv, 1.2) * 0.5 + ring(uv, 3.2) * 0.5;
}

void main() {
  // aberracao cromatica so no halo (equivale a ~2px da cena cheia)
  vec2 off = vec2(uTexel.x * 0.5, 0.0);
  vec3 glow = vec3(halo(vUv - off).r, halo(vUv).g, halo(vUv + off).b);
  // curva de fosforo: o halo cresce com o quadrado da luminancia,
  // entao os brancos estouram e os tracos fracos quase nao brilham
  float lum = dot(glow, vec3(0.299, 0.587, 0.114));
  vec3 shaped = glow * (0.3 + 2.1 * lum * lum);
  // em HDR o nucleo do halo estoura acima do branco SDR (fosforo
  // quente de verdade); em SDR uBoost = 0 e o resultado clampa
  vec3 hot = glow * glow * glow * uBoost;
  o = vec4(shaped * uGain + hot, 1.0);
}`

const SCALE = 4 // glow roda a 1/SCALE da resolucao da cena

export class CrtFx {
  private gl: WebGL2RenderingContext
  private tex: WebGLTexture
  private uTexel: WebGLUniformLocation
  private small: HTMLCanvasElement
  private smallCtx: CanvasRenderingContext2D
  private w = 0
  private h = 0
  private uGain: WebGLUniformLocation | null = null
  private uBoost: WebGLUniformLocation | null = null
  hdr = false

  constructor(private glowCanvas: HTMLCanvasElement) {
    const gl = glowCanvas.getContext('webgl2', {
      alpha: false,
      antialias: false,
      depth: false,
      stencil: false,
      powerPreference: 'high-performance',
    })
    if (!gl) throw new Error('webgl2 indisponivel')
    this.gl = gl

    const compile = (type: number, src: string): WebGLShader => {
      const s = gl.createShader(type)
      if (!s) throw new Error('shader')
      gl.shaderSource(s, src)
      gl.compileShader(s)
      if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
        throw new Error(gl.getShaderInfoLog(s) ?? 'erro de shader')
      }
      return s
    }
    const prog = gl.createProgram()
    if (!prog) throw new Error('program')
    gl.attachShader(prog, compile(gl.VERTEX_SHADER, VERT))
    gl.attachShader(prog, compile(gl.FRAGMENT_SHADER, FRAG))
    gl.linkProgram(prog)
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
      throw new Error(gl.getProgramInfoLog(prog) ?? 'erro de link')
    }
    gl.useProgram(prog)

    const tex = gl.createTexture()
    if (!tex) throw new Error('texture')
    this.tex = tex
    gl.bindTexture(gl.TEXTURE_2D, tex)
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true)
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR)
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR)
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE)
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE)

    gl.uniform1i(gl.getUniformLocation(prog, 'uScene'), 0)
    const uTexel = gl.getUniformLocation(prog, 'uTexel')
    if (!uTexel) throw new Error('uniform uTexel')
    this.uTexel = uTexel

    // HDR: em tela com headroom, backbuffer float16 em display-p3 +
    // modo extended range: o glow passa do branco SDR de verdade.
    // qualquer pedaco sem suporte -> segue SDR identico ao de sempre.
    try {
      const anyGl = gl as unknown as {
        drawingBufferStorage?: (fmt: number, w: number, h: number) => void
        drawingBufferColorSpace?: string
      }
      const anyCanvas = glowCanvas as unknown as {
        configureHighDynamicRange?: (o: { mode: string }) => void
      }
      if (
        matchMedia('(dynamic-range: high)').matches &&
        typeof anyGl.drawingBufferStorage === 'function'
      ) {
        anyGl.drawingBufferColorSpace = 'display-p3'
        anyCanvas.configureHighDynamicRange?.({ mode: 'extended' })
        this.hdr = true
      }
    } catch {
      this.hdr = false
    }
    this.uGain = gl.getUniformLocation(prog, 'uGain')
    this.uBoost = gl.getUniformLocation(prog, 'uBoost')
    this.applyRange()

    this.small = document.createElement('canvas')
    const c2d = this.small.getContext('2d', { alpha: false })
    if (!c2d) throw new Error('canvas 2d indisponivel')
    this.smallCtx = c2d
  }

  private applyRange(): void {
    this.gl.uniform1f(this.uGain, this.hdr ? 0.95 : 0.75)
    this.gl.uniform1f(this.uBoost, this.hdr ? 1.6 : 0.0)
  }

  present(scene: HTMLCanvasElement): void {
    const { gl } = this
    const w = Math.max(1, Math.floor(scene.width / SCALE))
    const h = Math.max(1, Math.floor(scene.height / SCALE))
    if (this.w !== w || this.h !== h) {
      this.w = w
      this.h = h
      this.small.width = w
      this.small.height = h
      this.glowCanvas.width = w
      this.glowCanvas.height = h
      if (this.hdr) {
        try {
          const anyGl = gl as unknown as {
            drawingBufferStorage: (fmt: number, w: number, h: number) => void
          }
          anyGl.drawingBufferStorage(gl.RGBA16F, w, h)
        } catch {
          this.hdr = false
          this.applyRange()
        }
      }
      gl.viewport(0, 0, w, h)
      gl.uniform2f(this.uTexel, 1 / w, 1 / h)
    }
    // downscale acelerado + upload pequeno (1/16 dos bytes da cena)
    this.smallCtx.drawImage(scene, 0, 0, w, h)
    gl.bindTexture(gl.TEXTURE_2D, this.tex)
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, this.small)
    gl.drawArrays(gl.TRIANGLES, 0, 3)
  }
}
