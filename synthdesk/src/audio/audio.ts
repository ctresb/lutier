import { spec } from '../components/registry'
import type { Graph } from '../core/graph'
import type { NodeState } from '../core/types'
import { PROCESSOR_JS, PROCESSOR_NAME } from './processor'

// audio da mesa. um AudioContext PERSISTENTE por speaker, cada um com a
// engine (AudioWorklet de processor.ts) rodando o dsp do patch inteiro.
// plugar/desplugar cabo NAO derruba nada: o subgrafo atras do speaker e
// serializado e mandado pra engine, que reconcilia com crossfade e
// mantem a fase dos osciladores. o contexto so morre quando o speaker
// sai da mesa.
//
// contexto nasce suspended sem gesto do usuario (politica de autoplay):
// o primeiro pointerdown/keydown da janela destrava todos.

const LEVEL = 0.16 // teto de seguranca por speaker

// playhead dos sequencers, reportado pela engine (passo exato, sem
// drift visual); o drawExtra do sequencer le daqui
export const seqSteps = new Map<number, number>()

interface EnginePatch {
  // ins[porta] = referencia {n: id da fonte, p: porta da fonte}
  nodes: {
    id: number
    type: string
    on: boolean
    ins: Record<string, { n: number; p: string }>
    params: Record<string, number>
  }[]
  out: number
  on: boolean
  level: number
}

interface Chain {
  ctx: AudioContext
  engine: AudioWorkletNode | null // null enquanto o modulo carrega
  device: number
  lastPatch: string // ultima serializacao enviada (dedup barato)
}

const workletUrl = URL.createObjectURL(
  new Blob([PROCESSOR_JS], { type: 'application/javascript' }),
)

export class DeskAudio {
  labels: string[] = ['DEFAULT']
  private ids: (string | null)[] = [null]
  private chains = new Map<number, Chain>()

  constructor() {
    void this.refresh()
    navigator.mediaDevices?.addEventListener?.('devicechange', () => void this.refresh())
    // gesto destrava contextos suspensos pela politica de autoplay
    const unlock = () => {
      for (const c of this.chains.values()) {
        if (c.ctx.state === 'suspended') void c.ctx.resume().catch(() => {})
      }
    }
    window.addEventListener('pointerdown', unlock, { capture: true })
    window.addEventListener('keydown', unlock, { capture: true })
  }

  private async refresh(): Promise<void> {
    try {
      const all = await navigator.mediaDevices.enumerateDevices()
      const outs = all.filter((d) => d.kind === 'audiooutput')
      if (outs.length === 0) return
      this.labels = outs.map((d, i) =>
        d.label ? d.label.toUpperCase() : `OUTPUT ${String(i + 1).padStart(2, '0')}`,
      )
      this.ids = outs.map((d) => d.deviceId || null)
    } catch {
      // sem permissao/suporte: fica so o DEFAULT
    }
  }

  count(): number {
    return this.labels.length
  }

  deviceLabel(i: number): string {
    return this.labels[((i % this.count()) + this.count()) % this.count()] ?? 'DEFAULT'
  }

  private deviceId(i: number): string | null {
    return this.ids[((i % this.count()) + this.count()) % this.count()] ?? null
  }

  // serializa o subgrafo atras do IN do speaker no formato da engine
  private serialize(graph: Graph, spk: NodeState): EnginePatch {
    const nodes: EnginePatch['nodes'] = []
    const seen = new Set<number>()
    const walk = (id: number): void => {
      if (seen.has(id)) return
      seen.add(id)
      const n = graph.node(id)
      if (!n) return
      const ins: Record<string, { n: number; p: string }> = {}
      for (const p of spec(n.type).inputs) {
        const c = graph.cables.find((k) => k.to.node === id && k.to.port === p.id)
        if (c) {
          ins[p.id] = { n: c.from.node, p: c.from.port }
          walk(c.from.node)
        }
      }
      nodes.push({
        id,
        type: n.type,
        on: (n.params.on ?? 1) > 0,
        ins,
        params: { ...n.params },
      })
    }
    const c = graph.cables.find((k) => k.to.node === spk.id && k.to.port === 'in')
    if (c) walk(c.from.node)
    return { nodes, out: c ? c.from.node : 0, on: (spk.params.on ?? 1) > 0, level: LEVEL }
  }

  private create(spkId: number): Chain {
    const ctx = new AudioContext()
    const chain: Chain = { ctx, engine: null, device: -1, lastPatch: '' }
    this.chains.set(spkId, chain)
    void ctx.audioWorklet
      .addModule(workletUrl)
      .then(() => {
        // speaker pode ter saido da mesa enquanto o modulo carregava
        if (this.chains.get(spkId) !== chain) return
        const engine = new AudioWorkletNode(ctx, PROCESSOR_NAME, {
          numberOfInputs: 0,
          outputChannelCount: [2],
        })
        // engine reporta o playhead dos sequencers (passo atual)
        engine.port.onmessage = (e) => {
          if (typeof e.data?.seq === 'number') seqSteps.set(e.data.seq, e.data.step)
        }
        engine.connect(ctx.destination)
        chain.engine = engine
        // manda o patch que chegou durante o carregamento
        if (chain.lastPatch) engine.port.postMessage(JSON.parse(chain.lastPatch))
      })
      .catch(() => {})
    return chain
  }

  private teardown(id: number): void {
    const chain = this.chains.get(id)
    if (chain) {
      void chain.ctx.close().catch(() => {})
      this.chains.delete(id)
    }
  }

  // reconcilia o som com o grafo; chamado a cada frame sujo. barato:
  // serializa o subgrafo e so fala com a engine quando algo mudou
  sync(graph: Graph): void {
    const alive = new Set<number>()

    for (const spk of graph.nodes) {
      if (spk.type !== 'speaker') continue
      alive.add(spk.id)
      const chain = this.chains.get(spk.id) ?? this.create(spk.id)
      if (chain.ctx.state === 'suspended') void chain.ctx.resume().catch(() => {})

      const dev = spk.params.device ?? 0
      if (dev !== chain.device) {
        chain.device = dev
        const id = this.deviceId(dev)
        const anyCtx = chain.ctx as unknown as { setSinkId?: (id: string) => Promise<void> }
        if (id && anyCtx.setSinkId) void anyCtx.setSinkId(id).catch(() => {})
      }

      const patch = this.serialize(graph, spk)
      const s = JSON.stringify(patch)
      if (s !== chain.lastPatch) {
        chain.lastPatch = s
        chain.engine?.port.postMessage(patch)
      }
    }

    for (const id of [...this.chains.keys()]) {
      if (!alive.has(id)) this.teardown(id)
    }
  }
}

export const deskAudio = new DeskAudio()
