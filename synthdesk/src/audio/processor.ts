// a ENGINE da mesa, como texto: este arquivo exporta o codigo js do
// AudioWorkletProcessor que roda todo o dsp do patch dentro do audio
// thread. vira Blob URL em audio.ts (nada de magica de bundler, funciona
// igual no vite dev, no build e no webview do tauri).
//
// por que uma engine e nao OscillatorNode/GainNode nativos:
// - o patch inteiro (osc, noise, reverb, device, sequencer, volume,
//   gain, channel, mix, math) e avaliado por amostra num unico
//   processor; qualquer out pluga em qualquer in, cv modula audio em
//   taxa de audio (a lei da mesa: tudo e tensao)
// - o caminho e ESTEREO: cada node avalia L (retorno) e R (memoR;
//   ausente = mono, R igual a L). channel faz balance de verdade
// - a fase de cada oscilador vive AQUI, indexada pelo id do node: plugar
//   ou desplugar cabo nao reinicia nada, o sinal ja estava correndo
//   (a "rede" da mesa, ver components/oscillator.ts)
// - mudanca de topologia faz crossfade de ~5ms entre o patch velho e o
//   novo: zero clique ao plugar cabo
// - ciclos de cabo resolvem com 1 amostra de atraso (feedback real de
//   mesa modular), nunca travam
// - DEVICE: instrumento tocavel - transpoe os osciladores do seu cone
//   de entrada (pilha de pitch, 2^((nota-60)/12)) e aplica ADSR; as
//   propriedades (attack/decay/sustain/release) vem do componente
//   envelope plugado no ENV
// - SEQUENCER: relogio proprio por node, 8 passos, manda o passo atual
//   de volta pro ui via port.postMessage (playhead exato, sem drift)
// - saw/square com polyblep (mesma tecnica da engine rust do lutier),
//   knobs suavizados (sem zipper), dc blocker e soft clip por canal
//
// cabos chegam como ins[porta] = { n: idDaFonte, p: portaDaFonte } -
// fontes multi-out (sequencer: note + gate) publicam as saidas extras
// no memo com chave 'id:porta'.

export const PROCESSOR_NAME = 'desk-engine'

export const PROCESSOR_JS = `
const XFADE = 256                       // ~5ms de crossfade de topologia
const clamp01 = (v) => (v < 0 ? 0 : v > 1 ? 1 : v)
// mesma curva de oitavas do knob (components/oscillator.ts)
const oscFreq = (v) => 20 * (Math.pow(2, clamp01(v) * 7) - 1)
// cv 0..1 -> nota midi 36..84 (mesma regua do sequencer e do device)
const cvNote = (v) => 36 + clamp01(v) * 48

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
    this.st = new Map()                 // id -> estado dsp
    this.prev = new Map()               // id -> out L da amostra anterior (ciclos)
    this.master = 0
    this.x1l = 0; this.y1l = 0          // dc blocker por canal
    this.x1r = 0; this.y1r = 0
    this.pitch = 1                      // pilha de transposicao dos devices
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

  // avalia uma porta de entrada: ref = {n, p}; retorna o L e deixa o
  // R correspondente em this.lastR (mono: R = L)
  inL(ref, nodes, memo, memoR, visiting) {
    if (!ref) { this.lastR = 0; return 0 }
    const l = this.evalNode(ref.n, nodes, memo, memoR, visiting)
    if (ref.p && ref.p !== 'out' && memo.has(ref.n + ':' + ref.p)) {
      const pv = memo.get(ref.n + ':' + ref.p)
      this.lastR = pv
      return pv
    }
    this.lastR = memoR.has(ref.n) ? memoR.get(ref.n) : l
    return l
  }

  evalNode(id, nodes, memo, memoR, visiting) {
    if (!id) return 0
    if (memo.has(id)) return memo.get(id)
    const n = nodes.get(id)
    if (!n || !n.on) return 0                       // desligado = inerte
    if (visiting.has(id)) return this.prev.get(id) ?? 0  // ciclo: 1 amostra de atraso
    visiting.add(id)
    let v = 0
    let r = null // saida R quando difere do L

    if (n.type === 'oscillator') {
      const s = this.state(n)
      if (n.ins.freq) {
        // cv no port FREQ em taxa de AUDIO: pot da altura, oscilador
        // da fm exponencial
        s.hz = oscFreq(this.inL(n.ins.freq, nodes, memo, memoR, visiting))
      } else {
        s.hz += this.kParam * (oscFreq(n.params.freq ?? 0) - s.hz)
      }
      // this.pitch = transposicao do device dono do cone (1 fora)
      const dt = Math.max((s.hz * this.pitch) / sampleRate, 1e-9)
      const t = s.ph
      const w = n.params.wave ?? 0
      if (w === 1) {                                // square + polyblep
        v = (t < 0.5 ? 1 : -1) + blep(t, dt) - blep((t + 0.5) % 1, dt)
      } else if (w === 2) {                         // triangle
        v = 4 * Math.abs(t - 0.5) - 1
      } else if (w === 3) {                         // saw + polyblep
        v = 2 * t - 1 - blep(t, dt)
      } else {                                      // sine
        v = Math.sin(t * 2 * Math.PI)
      }
      s.ph = (t + dt) % 1
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
      // density = chance por amostra de renovar o sample (decadas:
      // 1 = white pleno, 0.5 = lo-fi ~1.4khz, 0 = quase estalos)
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
      v *= this.sp(this.state(n), 'level', n.params.level ?? 0.8)
    } else if (n.type === 'volume') {
      const k = this.sp(this.state(n), 'value', n.params.value ?? 0)
      if (n.ins.in) {
        v = this.inL(n.ins.in, nodes, memo, memoR, visiting) * k
        r = this.lastR * k
      } else {
        v = k // sem nada no IN o knob vira fonte de tensao
      }
    } else if (n.type === 'gain') {
      // amplificador: knob 0..1 -> ganho 0..2 (0.5 = unitario)
      const k = this.sp(this.state(n), 'value', n.params.value ?? 0.5) * 2
      if (n.ins.in) {
        v = this.inL(n.ins.in, nodes, memo, memoR, visiting) * k
        r = this.lastR * k
      } else {
        v = k
      }
    } else if (n.type === 'channel') {
      // balance L/R: centro passa reto, extremo cala o outro lado
      const pan = this.sp(this.state(n), 'pan', n.params.pan ?? 0.5)
      const inl = n.ins.in ? this.inL(n.ins.in, nodes, memo, memoR, visiting) : 0
      const inr = n.ins.in ? this.lastR : 0
      v = inl * Math.min(1, (1 - pan) * 2)
      r = inr * Math.min(1, pan * 2)
    } else if (n.type === 'mix') {
      const s = this.state(n)
      const ka = this.sp(s, 'ka', n.params.ka ?? 0)
      const kb = this.sp(s, 'kb', n.params.kb ?? 0)
      let al = 0, ar = 0, bl = 0, br = 0
      if (n.ins.a) { al = this.inL(n.ins.a, nodes, memo, memoR, visiting); ar = this.lastR }
      if (n.ins.b) { bl = this.inL(n.ins.b, nodes, memo, memoR, visiting); br = this.lastR }
      v = al * ka + bl * kb
      const rr = ar * ka + br * kb
      if (rr !== v) r = rr
    } else if (n.type === 'math') {
      let al = 0, ar = 0, bl = 0, br = 0
      if (n.ins.a) { al = this.inL(n.ins.a, nodes, memo, memoR, visiting); ar = this.lastR }
      if (n.ins.b) { bl = this.inL(n.ins.b, nodes, memo, memoR, visiting); br = this.lastR }
      const op = (a, b) => {
        switch (n.params.op ?? 0) {
          case 1: return a - b
          case 2: return a * b
          case 3: return b === 0 ? 0 : a / b
          case 4: return Math.min(a, b)
          case 5: return Math.max(a, b)
          case 6: return (a + b) / 2
          default: return a + b
        }
      }
      v = op(al, bl)
      const rr = op(ar, br)
      if (rr !== v) r = rr
    } else if (n.type === 'sequencer') {
      const s = this.state(n)
      if (s.sph === undefined) { s.sph = 0; s.lastStep = -1 }
      const rate = clamp01(n.params.rate ?? 0.5)
      const hz = Math.pow(2, rate * 4) // 1..16 passos/s
      s.sph += hz / sampleRate
      const pos = s.sph % 8
      const step = Math.floor(pos)
      if (step !== s.lastStep) {
        s.lastStep = step
        // playhead exato de volta pro ui (sem drift visual)
        this.port.postMessage({ seq: n.id, step })
      }
      const active = (n.params['step' + (step + 1)] ?? 0) > 0.5
      const gate = active && pos - step < 0.8 ? 1 : 0 // duty 80%: re-articula
      // saidas: default = gate; note publicada como porta extra
      v = gate
      memo.set(id + ':gate', gate)
      memo.set(id + ':note', clamp01(n.params.pitch ?? 0.5))
    } else if (n.type === 'device') {
      const s = this.state(n)
      if (s.lv === undefined) { s.lv = 0; s.stage = 0; s.g = 0 }
      // nota e gate: cabo ganha do param (teclado/vars escrevem no param)
      const gate = n.ins.gate
        ? (this.inL(n.ins.gate, nodes, memo, memoR, visiting) > 0.5 ? 1 : 0)
        : ((n.params.gate ?? 0) > 0.5 ? 1 : 0)
      const note = n.ins.note
        ? cvNote(this.inL(n.ins.note, nodes, memo, memoR, visiting))
        : (n.params.note ?? 60)
      // propriedades: componente envelope plugado no ENV (parametros
      // lidos direto - propriedade e descricao, nao sinal)
      let atk = 0.004, dec = 0.05, sus = 1, rel = 0.03
      const env = n.ins.env ? nodes.get(n.ins.env.n) : null
      if (env && env.type === 'envelope' && env.on) {
        atk = 0.002 + Math.pow(env.params.attack ?? 0.05, 2) * 2
        dec = 0.005 + Math.pow(env.params.decay ?? 0.3, 2) * 2
        sus = clamp01(env.params.sustain ?? 0.7)
        rel = 0.005 + Math.pow(env.params.release ?? 0.25, 2) * 3
      }
      // adsr: ataque/release lineares, decay exponencial pro sustain
      if (gate > 0 && s.g === 0) s.stage = 1
      if (gate === 0 && s.g === 1) s.stage = 4
      s.g = gate
      if (s.stage === 1) {
        s.lv += 1 / (atk * sampleRate)
        if (s.lv >= 1) { s.lv = 1; s.stage = 2 }
      } else if (s.stage === 2) {
        s.lv += (sus - s.lv) * (1 - Math.exp(-1 / (dec * sampleRate)))
        if (Math.abs(s.lv - sus) < 1e-3) s.stage = 3
      } else if (s.stage === 3) {
        s.lv = sus
      } else if (s.stage === 4) {
        s.lv -= 1 / (rel * sampleRate)
        if (s.lv <= 0) { s.lv = 0; s.stage = 0 }
      } else {
        s.lv = 0
      }
      // transposicao do cone de entrada: osciladores rio acima tocam
      // a nota (pilha: device dentro de device compoe)
      const keep = this.pitch
      this.pitch = keep * Math.pow(2, (note - 60) / 12)
      const il = n.ins.in ? this.inL(n.ins.in, nodes, memo, memoR, visiting) : 0
      const ir = n.ins.in ? this.lastR : 0
      this.pitch = keep
      v = il * s.lv
      if (ir !== il) r = ir * s.lv
    } else if (n.type === 'envelope') {
      // propriedade de device; como sinal, emite o sustain (dc)
      v = clamp01(n.params.sustain ?? 0.7)
    } else if (n.type === 'reverb') {
      const s = this.state(n)
      // entrada mono-izada (reverb da mesa e mono, sala unica)
      let x = 0
      if (n.ins.in) {
        const l = this.inL(n.ins.in, nodes, memo, memoR, visiting)
        x = (l + this.lastR) * 0.5
      }
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
      let rv = acc / s.combs.length
      for (const a of s.aps) {
        // allpass em serie difunde a cauda
        const b = a.buf[a.i]
        const y = b - 0.5 * rv
        a.buf[a.i] = rv + 0.5 * y
        a.i = (a.i + 1) % a.buf.length
        rv = y
      }
      const dry = this.sp(s, 'dry', n.params.dry ?? 0.8)
      const wet = this.sp(s, 'wet', n.params.wet ?? 0.35)
      v = x * dry + rv * wet
    }

    if (!Number.isFinite(v)) v = 0
    memo.set(id, v)
    if (r !== null && Number.isFinite(r) && r !== v) memoR.set(id, r)
    visiting.delete(id)
    return v
  }

  process(_inputs, outputs) {
    const out = outputs[0]
    const ch0 = out && out[0]
    if (!ch0) return true
    const ch1 = out[1] ?? ch0
    const tgt = this.on ? this.level : 0
    const rootRef = { n: this.out, p: 'out' }
    const oldRef = { n: this.oldOut, p: 'out' }
    for (let i = 0; i < ch0.length; i++) {
      const memo = new Map()
      const memoR = new Map()
      let l = this.inL(rootRef, this.nodes, memo, memoR, new Set())
      let r = this.lastR
      if (this.xfade > 0 && this.oldNodes) {
        // memo compartilhado: node presente nos dois patches avalia (e
        // avanca fase) UMA vez so
        const lo = this.inL(oldRef, this.oldNodes, memo, memoR, new Set())
        const ro = this.lastR
        const f = this.xfade / XFADE
        l = lo * f + l * (1 - f)
        r = ro * f + r * (1 - f)
        this.xfade--
        if (this.xfade === 0) this.oldNodes = null
      }
      for (const [k, val] of memo) {
        if (typeof k === 'number') this.prev.set(k, val)
      }
      // dc blocker por canal: pot plugado direto no speaker e tensao
      // continua legitima na mesa, mas nao vira dc no alto-falante
      const yl = l - this.x1l + 0.9995 * this.y1l
      this.x1l = l; this.y1l = yl
      const yr = r - this.x1r + 0.9995 * this.y1r
      this.x1r = r; this.y1r = yr
      this.master += this.kMaster * (tgt - this.master)
      // soft clip com joelho em 0.5: nivel nominal passa reto,
      // sobrecarga comprime em vez de estourar
      ch0[i] = Math.tanh(yl * this.master * 2) * 0.5
      ch1[i] = Math.tanh(yr * this.master * 2) * 0.5
    }
    for (let c = 2; c < out.length; c++) out[c].set(ch0)
    return true
  }
}

registerProcessor('desk-engine', DeskEngine)
`
