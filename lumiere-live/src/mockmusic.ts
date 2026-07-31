// mock de dev: uma MUSICA de verdade, sintetizada em webaudio
// (kick, baixo, hats, pad) com secoes calm -> groove -> peak em
// loop, analisada pelo mesmo perfil do backend rust. o master e
// mudo (gain 0): so o analisador escuta. resultado: goniometro,
// espectro e beat organicos no navegador, sem asset nenhum.

import { AudioFrame, SPEC_N, WAVE_N, GONIO_N } from './audio'

const BPM = 124
const BEAT = 60 / BPM

// intensidade por compasso (loop de 32 bars): chill, sobe, poca, cai
function intensityAt(bar: number): number {
  const b = bar % 32
  if (b < 8) return 0.12
  if (b < 12) return 0.3 + (b - 8) * 0.08
  if (b < 20) return 0.62
  if (b < 26) return 0.95
  if (b < 28) return 0.5
  return 0.2
}

const BASS_SEQ = [0, 0, 7, 0, 3, 3, 5, 7]
const PAD_CHORD = [0, 3, 7, 10]

function midiHz(root: number, semi: number): number {
  return 55 * 2 ** ((root + semi) / 12)
}

export function startMockMusic(onFrame: (f: AudioFrame) => void): void {
  const ctx = new AudioContext({ sampleRate: 48000 })
  const bus = ctx.createGain()
  bus.gain.value = 1
  const splitter = ctx.createChannelSplitter(2)
  const anL = ctx.createAnalyser()
  const anR = ctx.createAnalyser()
  anL.fftSize = 2048
  anR.fftSize = 2048
  anL.smoothingTimeConstant = 0.25
  anR.smoothingTimeConstant = 0.25
  const mute = ctx.createGain()
  mute.gain.value = 0
  bus.connect(splitter)
  splitter.connect(anL, 0)
  splitter.connect(anR, 1)
  bus.connect(mute)
  mute.connect(ctx.destination)

  // ruido pros hats
  const noiseBuf = ctx.createBuffer(1, 48000, 48000)
  const nd = noiseBuf.getChannelData(0)
  for (let i = 0; i < nd.length; i++) nd[i] = Math.random() * 2 - 1

  const kick = (at: number, vel: number) => {
    const o = ctx.createOscillator()
    const g = ctx.createGain()
    o.frequency.setValueAtTime(150, at)
    o.frequency.exponentialRampToValueAtTime(48, at + 0.09)
    g.gain.setValueAtTime(0.9 * vel, at)
    g.gain.exponentialRampToValueAtTime(0.001, at + 0.22)
    o.connect(g)
    g.connect(bus)
    o.start(at)
    o.stop(at + 0.3)
  }
  const bass = (at: number, hz: number, vel: number, dur: number) => {
    const o = ctx.createOscillator()
    o.type = 'sawtooth'
    o.frequency.value = hz
    const f = ctx.createBiquadFilter()
    f.type = 'lowpass'
    f.frequency.setValueAtTime(180 + vel * 700, at)
    f.frequency.exponentialRampToValueAtTime(140, at + dur)
    const g = ctx.createGain()
    g.gain.setValueAtTime(0.34 * vel, at)
    g.gain.setTargetAtTime(0.0001, at + dur * 0.7, 0.05)
    o.connect(f)
    f.connect(g)
    g.connect(bus)
    o.start(at)
    o.stop(at + dur + 0.2)
  }
  const hat = (at: number, vel: number, panv: number) => {
    const s = ctx.createBufferSource()
    s.buffer = noiseBuf
    const f = ctx.createBiquadFilter()
    f.type = 'highpass'
    f.frequency.value = 7000
    const g = ctx.createGain()
    g.gain.setValueAtTime(0.16 * vel, at)
    g.gain.exponentialRampToValueAtTime(0.001, at + 0.045)
    const p = ctx.createStereoPanner()
    p.pan.value = panv
    s.connect(f)
    f.connect(g)
    g.connect(p)
    p.connect(bus)
    s.start(at, Math.random() * 0.4, 0.08)
  }
  const padVoices: OscillatorNode[] = []
  const padGain = ctx.createGain()
  padGain.gain.value = 0.05
  const padFilter = ctx.createBiquadFilter()
  padFilter.type = 'lowpass'
  padFilter.frequency.value = 900
  padGain.connect(padFilter)
  padFilter.connect(bus)
  PAD_CHORD.forEach((semi, i) => {
    const o = ctx.createOscillator()
    o.type = 'sawtooth'
    o.frequency.value = midiHz(24, semi)
    o.detune.value = (i - 1.5) * 9
    const p = ctx.createStereoPanner()
    p.pan.value = (i - 1.5) * 0.4
    o.connect(p)
    p.connect(padGain)
    o.start()
    padVoices.push(o)
  })
  void padVoices

  // agendador com lookahead
  let nextBeat = 0
  let beatCount = 0
  setInterval(() => {
    if (ctx.state === 'suspended') { void ctx.resume(); return }
    const ahead = ctx.currentTime + 0.25
    while (nextBeat < ahead) {
      const at = Math.max(nextBeat, ctx.currentTime + 0.01)
      const bar = Math.floor(beatCount / 4)
      const beatInBar = beatCount % 4
      const inten = intensityAt(bar)
      if (inten > 0.25 || beatInBar % 2 === 0) kick(at, 0.4 + inten * 0.6)
      const semi = BASS_SEQ[beatCount % BASS_SEQ.length]
      if (inten > 0.2) {
        bass(at, midiHz(12, semi), 0.3 + inten * 0.7, BEAT * 0.9)
        bass(at + BEAT * 0.5, midiHz(12, semi), (0.3 + inten * 0.7) * 0.6, BEAT * 0.4)
      }
      if (inten > 0.45) {
        hat(at + BEAT * 0.5, 0.6 + inten * 0.4, beatCount % 2 ? 0.5 : -0.5)
      }
      if (inten > 0.8) {
        hat(at + BEAT * 0.25, 0.4, -0.3)
        hat(at + BEAT * 0.75, 0.4, 0.3)
      }
      padGain.gain.setTargetAtTime(0.02 + (1 - inten) * 0.07 + inten * 0.02, at, 0.4)
      padFilter.frequency.setTargetAtTime(500 + inten * 2600, at, 0.3)
      nextBeat += BEAT
      beatCount++
    }
  }, 100)

  // ---- analise (mesmo perfil do backend rust) ----
  const N = 2048
  const tdL = new Float32Array(N)
  const tdR = new Float32Array(N)
  const fdL = new Float32Array(anL.frequencyBinCount)
  const fdR = new Float32Array(anR.frequencyBinCount)
  const spec = new Uint8Array(SPEC_N)
  const wave = new Int8Array(WAVE_N)
  const gonio = new Int8Array(GONIO_N * 2)
  const prev = new Float32Array(SPEC_N)
  const sr = 48000
  const edges = new Float32Array(SPEC_N + 1)
  const fHi = Math.min(18000, sr * 0.45)
  for (let k = 0; k <= SPEC_N; k++) edges[k] = 28 * (fHi / 28) ** (k / SPEC_N)

  setInterval(() => {
    anL.getFloatTimeDomainData(tdL)
    anR.getFloatTimeDomainData(tdR)
    anL.getFloatFrequencyData(fdL)
    anR.getFloatFrequencyData(fdR)

    let rms = 0
    let peak = 0
    let sl = 0; let srr = 0; let slr = 0
    for (let i = 0; i < N; i++) {
      const m = 0.5 * (tdL[i] + tdR[i])
      rms += m * m
      peak = Math.max(peak, Math.abs(m))
      sl += tdL[i] * tdL[i]
      srr += tdR[i] * tdR[i]
      slr += tdL[i] * tdR[i]
    }
    rms = Math.sqrt(rms / N)
    const crest = rms > 1e-6 ? peak / rms : 0
    const corr = slr / (Math.sqrt(sl) * Math.sqrt(srr) + 1e-9)
    const width = Math.min(Math.max((1 - corr) * 0.5, 0), 1)

    const binHz = sr / 2 / fdL.length
    let flux = 0
    let num = 0; let den = 0
    const band = [0, 0, 0]
    const bandn = [0, 0, 0]
    for (let k = 0; k < SPEC_N; k++) {
      const b0 = Math.max(1, Math.floor(edges[k] / binHz))
      const b1 = Math.max(b0 + 1, Math.floor(edges[k + 1] / binHz))
      let db = -120
      for (let i = b0; i < Math.min(b1, fdL.length); i++) {
        db = Math.max(db, 0.5 * (fdL[i] + fdR[i]))
      }
      const v = Math.min(Math.max((db + 76) / 62, 0), 1)
      flux += Math.max(0, v - prev[k])
      prev[k] = v
      spec[k] = (v * 255) | 0
      const fc = edges[k]
      const bi = fc < 150 ? 0 : fc < 2000 ? 1 : 2
      band[bi] += v
      bandn[bi]++
      num += fc * v
      den += v
    }
    flux = Math.min((flux / SPEC_N) * 8, 1)
    for (let i = 0; i < 3; i++) band[i] /= Math.max(bandn[i], 1)

    const step = N / WAVE_N
    for (let k = 0; k < WAVE_N; k++) {
      let acc = 0
      for (let i = 0; i < step; i++) acc += 0.5 * (tdL[k * step + i] + tdR[k * step + i])
      wave[k] = Math.max(-127, Math.min(127, (acc / step) * 1.6 * 127)) | 0
    }
    for (let k = 0; k < GONIO_N; k++) {
      const i = N - GONIO_N + k
      gonio[k * 2] = Math.max(-127, Math.min(127,
        (tdL[i] - tdR[i]) * 0.7071 * 2.4 * 127)) | 0
      gonio[k * 2 + 1] = Math.max(-127, Math.min(127,
        (tdL[i] + tdR[i]) * 0.7071 * 2.4 * 127)) | 0
    }

    onFrame({
      spec, wave, gonio, rms, peak,
      centroid: den > 1e-9 ? num / den : 0,
      flux, crest, width,
      low: band[0], mid: band[1], high: band[2], sr,
    })
  }, 1000 / 60)

  // autoplay: destrava no primeiro clique se o contexto nascer preso
  const unlock = () => { void ctx.resume() }
  window.addEventListener('click', unlock, { once: true })
  ;(window as unknown as { __mockCtx: AudioContext }).__mockCtx = ctx
}
