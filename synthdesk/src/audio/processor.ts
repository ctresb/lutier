// a ENGINE da mesa, como texto: este arquivo exporta o codigo js do
// AudioWorkletProcessor que roda todo o dsp do patch dentro do audio
// thread. vira Blob URL em audio.ts (nada de magica de bundler, funciona
// igual no vite dev, no build e no webview do tauri).
//
// por que uma engine e nao OscillatorNode/GainNode nativos:
// - o patch inteiro (osc, noise, reverb, volume, mix, math) e avaliado
//   por amostra num unico processor; qualquer out pluga em qualquer in,
//   cv modula audio em taxa de audio (a lei da mesa: tudo e tensao)
// - a fase de cada oscilador vive AQUI, indexada pelo id do node: plugar
//   ou desplugar cabo nao reinicia nada, o sinal ja estava correndo
//   (a "rede" da mesa, ver components/oscillator.ts)
// - mudanca de topologia faz crossfade de ~5ms entre o patch velho e o
//   novo: zero clique ao plugar cabo
// - ciclos de cabo resolvem com 1 amostra de atraso (feedback real de
//   mesa modular), nunca travam
// - saw/square com polyblep (mesma tecnica da engine rust do lutier),
//   knobs suavizados (sem zipper), dc blocker e soft clip na saida

export const PROCESSOR_NAME = 'desk-engine'

export const PROCESSOR_JS = `
const XFADE = 256                       // ~5ms de crossfade de topologia
const clamp01 = (v) => (v < 0 ? 0 : v > 1 ? 1 : v)
// mesma curva de oitavas do knob (components/oscillator.ts)
const oscFreq = (v) => 20 * (Math.pow(2, clamp01(v) * 7) - 1)

// polyblep: suaviza a descontinuidade de saw/square na largura de 1
// amostra; mata o aliasing sem mudar o timbre
function blep(t, dt) {
  if (t < dt) { t /= dt; return t + t - t * t - 1 }
  if (t > 1 - dt) { t = (t - 1) / dt; return t * t + t + t + 1 }
  return 0
}

class DeskEngine extends AudioWorkletProcessor {
  constructor() {
    super()
    this.nodes = new Map()              // id -> {type, on, ins, params}
    this.out = 0                        // id da fonte plugada no speaker
    this.on = false
    this.level = 0
    this.sig = ''
    this.oldNodes = null                // patch anterior durante o crossfade
    this.oldOut = 0
    this.xfade = 0
    this.st = new Map()                 // id -> estado dsp {ph, hz, p}
    this.prev = new Map()               // id -> out da amostra anterior (ciclos)
    this.master = 0
    this.x1 = 0                         // dc blocker
    this.y1 = 0
    this.kParam = 1 - Math.exp(-1 / (0.015 * sampleRate))
    this.kMaster = 1 - Math.exp(-1 / (0.008 * sampleRate))
    this.port.onmessage = (e) => this.patch(e.data)
  }

  patch(m) {
    const nodes = new Map()
    for (const n of m.nodes) nodes.set(n.id, n)
    // assinatura so de TOPOLOGIA (ids, tipos, on/off, cabos): knob
    // girando nao dispara crossfade, cabo plugado sim
    const sig = m.nodes
      .map((n) => n.id + ':' + n.type + ':' + (n.on ? 1 : 0) + ':' + JSON.stringify(n.ins))
      .join('|') + '>' + m.out
    if (this.sig !== '' && sig !== this.sig) {
      this.oldNodes = this.nodes
      this.oldOut = this.out
      this.xfade = XFADE
    }
    this.sig = sig
    this.nodes = nodes
    this.out = m.out
    this.on = m.on
    this.level = m.level
    // estado de quem saiu da mesa morre (fase nao importa mais)
    for (const id of [...this.st.keys()]) {
      if (!nodes.has(id) && !(this.oldNodes && this.oldNodes.has(id))) {
        this.st.delete(id)
        this.prev.delete(id)
      }
    }
  }

  state(n) {
    let s = this.st.get(n.id)
    if (!s) {
      // fase engata na REDE: currentFrame e o relogio comum da mesa,
      // entao dois osciladores na mesma freq nascem alinhados, plugue
      // quando plugar
      const hz = oscFreq(n.params.freq ?? 0)
      s = { ph: (hz * currentFrame / sampleRate) % 1, hz, p: {} }
      this.st.set(n.id, s)
    }
    return s
  }

  // knob suavizado (~15ms): girar nao da zipper nem degrau
  sp(s, id, target) {
    let c = s.p[id]
    if (c === undefined) c = target
    c += this.kParam * (target - c)
    s.p[id] = c
    return c
  }

  evalNode(id, nodes, memo, visiting) {
    if (!id) return 0
    if (memo.has(id)) return memo.get(id)
    const n = nodes.get(id)
    if (!n || !n.on) return 0                       // desligado = inerte
    if (visiting.has(id)) return this.prev.get(id) ?? 0  // ciclo: 1 amostra de atraso
    visiting.add(id)
    let v = 0

    if (n.type === 'oscillator') {
      const s = this.state(n)
      if (n.ins.freq) {
        // cv no port FREQ em taxa de AUDIO: pot da altura, oscilador
        // da fm exponencial de graca
        s.hz = oscFreq(this.evalNode(n.ins.freq, nodes, memo, visiting))
      } else {
        s.hz += this.kParam * (oscFreq(n.params.freq ?? 0) - s.hz)
      }
      const dt = Math.max(s.hz / sampleRate, 1e-9)
      const t = s.ph
      const w = n.params.wave ?? 0
      if (w === 1) {                                // square + polyblep
        v = (t < 0.5 ? 1 : -1) + blep(t, dt) - blep((t + 0.5) % 1, dt)
      } else if (w === 2) {                         // triangle (harmonicos caem 12db/oit, alias desprezivel no range da mesa)
        v = 4 * Math.abs(t - 0.5) - 1
      } else if (w === 3) {                         // saw + polyblep
        v = 2 * t - 1 - blep(t, dt)
      } else {                                      // sine
        v = Math.sin(t * 2 * Math.PI)
      }
      s.ph = (t + dt) % 1
    } else if (n.type === 'volume') {
      const k = this.sp(this.state(n), 'value', n.params.value ?? 0)
      // sem nada no IN o knob vira fonte de tensao (mesma lei do cvOut)
      v = n.ins.in ? this.evalNode(n.ins.in, nodes, memo, visiting) * k : k
    } else if (n.type === 'mix') {
      const s = this.state(n)
      const a = n.ins.a ? this.evalNode(n.ins.a, nodes, memo, visiting) : 0
      const b = n.ins.b ? this.evalNode(n.ins.b, nodes, memo, visiting) : 0
      v = a * this.sp(s, 'ka', n.params.ka ?? 0) + b * this.sp(s, 'kb', n.params.kb ?? 0)
    } else if (n.type === 'math') {
      const a = n.ins.a ? this.evalNode(n.ins.a, nodes, memo, visiting) : 0
      const b = n.ins.b ? this.evalNode(n.ins.b, nodes, memo, visiting) : 0
      switch (n.params.op ?? 0) {
        case 1: v = a - b; break
        case 2: v = a * b; break
        case 3: v = b === 0 ? 0 : a / b; break
        case 4: v = Math.min(a, b); break
        case 5: v = Math.max(a, b); break
        case 6: v = (a + b) / 2; break
        default: v = a + b
      }
    } else if (n.type === 'noise') {
      const s = this.state(n)
      if (s.rng === undefined) {
        s.rng = ((n.id * 2654435761) ^ 0x9e3779b9) >>> 0 || 1
        s.b0 = 0; s.b1 = 0; s.b2 = 0; s.brown = 0; s.held = 0
      }
      s.rng ^= s.rng << 13; s.rng >>>= 0
      s.rng ^= s.rng >>> 17
      s.rng ^= s.rng << 5; s.rng >>>= 0
      const u = s.rng / 4294967296
      // density = chance por amostra de renovar o sample (mapeada em
      // decadas: 1 = white pleno, 0.5 = lo-fi granulado ~1.4khz,
      // 0 = quase estalos); segura o ultimo valor no resto
      const density = clamp01(n.params.density ?? 1)
      const p = Math.pow(10, (density - 1) * 3)
      if (u < p || density >= 1) {
        // segundo avanco pro VALOR: decidir e sortear com o mesmo
        // numero enviesaria o sample pra baixo
        s.rng ^= s.rng << 13; s.rng >>>= 0
        s.rng ^= s.rng >>> 17
        s.rng ^= s.rng << 5; s.rng >>>= 0
        s.held = s.rng / 2147483648 - 1
      }
      const w = s.held
      const t = n.params.type ?? 0
      if (t === 1) {
        // pink: kellet economico (3 estagios), -3db/oitava
        s.b0 = 0.99765 * s.b0 + w * 0.099046
        s.b1 = 0.963 * s.b1 + w * 0.2965164
        s.b2 = 0.57 * s.b2 + w * 1.0526913
        v = (s.b0 + s.b1 + s.b2 + w * 0.1848) * 0.22
      } else if (t === 2) {
        // brown: integrador com vazamento, -6db/oitava
        s.brown = (s.brown + 0.02 * w) * 0.998
        v = s.brown * 2.5
      } else {
        v = w * 0.8 // white
      }
      v *= this.sp(s, 'level', n.params.level ?? 0.8)
    } else if (n.type === 'reverb') {
      const s = this.state(n)
      const x = n.ins.in ? this.evalNode(n.ins.in, nodes, memo, visiting) : 0
      // walls chega normalizado 0..1 (slider com 6 detents) -> 3..8
      const walls = 3 + Math.round(clamp01(n.params.walls ?? 0.4) * 5)
      const type = n.params.type ?? 0
      const cfg = walls * 10 + type
      if (s.rvCfg !== cfg) {
        // paredes/tipo mudou: reconstroi as linhas de atraso (schroeder:
        // 1 comb por parede, delays primos escalados pelo tipo)
        s.rvCfg = cfg
        s.rvRt = -1
        const scale = [0.75, 1.4, 0.5, 2.2][type] ?? 1
        const base = [0.0297, 0.0371, 0.0411, 0.0437, 0.0479, 0.0533, 0.0599, 0.0614]
        s.combs = []
        for (let i = 0; i < walls; i++) {
          const len = Math.max(1, Math.round(base[i] * scale * sampleRate))
          s.combs.push({ buf: new Float64Array(len), i: 0, lp: 0 })
        }
        s.aps = [0.0051, 0.0017].map((d) => ({
          buf: new Float64Array(Math.max(1, Math.round(d * sampleRate))),
          i: 0,
        }))
      }
      const speed = this.sp(s, 'speed', n.params.speed ?? 0.5)
      // speed = quao rapido a cauda morre: rt60 de ~4.5s (0) a ~0.35s (1)
      const rt60 = (0.35 + (1 - speed) * 4.2) * ([0.8, 1.3, 0.6, 1.8][type] ?? 1)
      if (s.rvRt !== rt60) {
        s.rvRt = rt60
        s.fbs = s.combs.map((c) => Math.pow(10, (-3 * (c.buf.length / sampleRate)) / rt60))
      }
      const damp = [0.35, 0.5, 0.15, 0.65][type] ?? 0.4
      let acc = 0
      for (let i = 0; i < s.combs.length; i++) {
        const c = s.combs[i]
        const out = c.buf[c.i]
        c.lp = out * (1 - damp) + c.lp * damp // abafamento no loop
        c.buf[c.i] = x + c.lp * s.fbs[i]
        c.i = (c.i + 1) % c.buf.length
        acc += out
      }
      let r = acc / s.combs.length
      for (const a of s.aps) {
        // allpass em serie difunde a cauda
        const b = a.buf[a.i]
        const y = b - 0.5 * r
        a.buf[a.i] = r + 0.5 * y
        a.i = (a.i + 1) % a.buf.length
        r = y
      }
      const dry = this.sp(s, 'dry', n.params.dry ?? 0.8)
      const wet = this.sp(s, 'wet', n.params.wet ?? 0.35)
      v = x * dry + r * wet
    }

    if (!Number.isFinite(v)) v = 0
    memo.set(id, v)
    visiting.delete(id)
    return v
  }

  process(_inputs, outputs) {
    const out = outputs[0]
    const ch0 = out && out[0]
    if (!ch0) return true
    const tgt = this.on ? this.level : 0
    for (let i = 0; i < ch0.length; i++) {
      const memo = new Map()
      let v = this.evalNode(this.out, this.nodes, memo, new Set())
      if (this.xfade > 0 && this.oldNodes) {
        // memo compartilhado: node presente nos dois patches avalia (e
        // avanca fase) UMA vez so
        const vo = this.evalNode(this.oldOut, this.oldNodes, memo, new Set())
        const f = this.xfade / XFADE
        v = vo * f + v * (1 - f)
        this.xfade--
        if (this.xfade === 0) this.oldNodes = null
      }
      for (const [id, val] of memo) this.prev.set(id, val)
      // dc blocker: pot plugado direto no speaker e tensao continua
      // legitima na mesa, mas nao vira dc no alto-falante
      const y = v - this.x1 + 0.9995 * this.y1
      this.x1 = v
      this.y1 = y
      this.master += this.kMaster * (tgt - this.master)
      // soft clip com joelho em 0.5: nivel nominal passa reto,
      // sobrecarga comprime em vez de estourar
      ch0[i] = Math.tanh(y * this.master * 2) * 0.5
    }
    for (let c = 1; c < out.length; c++) out[c].set(ch0)
    return true
  }
}

registerProcessor('desk-engine', DeskEngine)
`
