// ponte de audio: no tauri o backend (cpal + fft em rust) emite o
// evento 'af' ~60hz com um frame binario base64; fora do tauri
// (vite puro no navegador) roda um mock procedural pra dev visual.

export const SPEC_N = 216
export const WAVE_N = 480
export const GONIO_N = 512

// layout do frame: [spec u8][wave i8][gonio i8 x2][10 f32 le]
const OFF_WAVE = SPEC_N
const OFF_GONIO = OFF_WAVE + WAVE_N
const OFF_METRICS = OFF_GONIO + GONIO_N * 2
export const FRAME_BYTES = OFF_METRICS + 10 * 4

export interface AudioFrame {
  spec: Uint8Array        // 216 bins log 28hz..18khz, 0..255
  wave: Int8Array         // 480 amostras mono -127..127
  gonio: Int8Array        // 512 pares (x, y) -127..127
  rms: number
  peak: number
  centroid: number        // hz
  flux: number            // 0..1
  crest: number
  width: number           // 0..1 (0 = mono travado)
  low: number
  mid: number
  high: number
  sr: number
}

export function decodeFrame(b64: string): AudioFrame | null {
  const raw = atob(b64)
  if (raw.length < FRAME_BYTES) return null
  const bytes = new Uint8Array(FRAME_BYTES)
  for (let i = 0; i < FRAME_BYTES; i++) bytes[i] = raw.charCodeAt(i)
  const dv = new DataView(bytes.buffer, OFF_METRICS)
  return {
    spec: bytes.subarray(0, SPEC_N),
    wave: new Int8Array(bytes.buffer, OFF_WAVE, WAVE_N),
    gonio: new Int8Array(bytes.buffer, OFF_GONIO, GONIO_N * 2),
    rms: dv.getFloat32(0, true),
    peak: dv.getFloat32(4, true),
    centroid: dv.getFloat32(8, true),
    flux: dv.getFloat32(12, true),
    crest: dv.getFloat32(16, true),
    width: dv.getFloat32(20, true),
    low: dv.getFloat32(24, true),
    mid: dv.getFloat32(28, true),
    high: dv.getFloat32(32, true),
    sr: dv.getFloat32(36, true),
  }
}

export const inTauri = '__TAURI_INTERNALS__' in window

export interface AudioBridge {
  devices: string[]
  current: number
  onFrame: (f: AudioFrame) => void
  onError?: (msg: string) => void
  cycle(dir: number): Promise<void>
}

export async function connect(onFrame: (f: AudioFrame) => void): Promise<AudioBridge> {
  if (inTauri) {
    const { listen } = await import('@tauri-apps/api/event')
    const { invoke } = await import('@tauri-apps/api/core')
    const bridge: AudioBridge = {
      devices: [],
      current: 0,
      onFrame,
      async cycle(dir: number) {
        this.devices = await invoke<string[]>('list_inputs')
        if (!this.devices.length) return
        this.current = (this.current + dir + this.devices.length) % this.devices.length
        try {
          await invoke('start_capture', { name: this.devices[this.current] })
        } catch (e) {
          console.error('start_capture falhou:', e)
        }
      },
    }
    await listen<string>('af', (e) => {
      const f = decodeFrame(e.payload)
      if (f) bridge.onFrame(f)
    })
    await listen<string>('af_err', (e) => {
      console.error('captura caiu:', e.payload)
      bridge.onError?.(e.payload)
    })
    try {
      bridge.devices = await invoke<string[]>('list_inputs')
      const def = await invoke<string>('start_default')
      const i = bridge.devices.indexOf(def)
      bridge.current = i >= 0 ? i : 0
    } catch { /* sem device: fica quieto */ }
    return bridge
  }
  return mockBridge(onFrame)
}

// ---------- mock pro navegador (dev sem backend) ----------

function mockBridge(onFrame: (f: AudioFrame) => void): AudioBridge {
  const bridge: AudioBridge = {
    devices: ['MOCK MUSIC ENGINE'],
    current: 0,
    onFrame,
    async cycle() { /* um device so */ },
  }
  // musica sintetizada de verdade + analise identica ao backend
  void import('./mockmusic').then((m) => m.startMockMusic(onFrame))
  return bridge
}
