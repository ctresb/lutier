import { spec } from '../components/registry'
import { setNodeCount, setSnap, setSubject, setZoom } from '../ui/hud'
import type { Camera } from './camera'
import type { Graph, GraphData } from './graph'
import { settings } from './settings'

// projeto .synthproj: um json que salva TUDO da mesa - componentes
// (posicao, nome, params, lock), cabos com waypoints, contadores de
// id, camera (pan/zoom) e snap. save/load por arquivo (dialog nativo
// no tauri, file system access no browser) + autosave: a cada 2s o
// estado vai pro localStorage (recuperacao de sessao) e, se o projeto
// ja tem arquivo, pro proprio arquivo tambem.

export interface ProjectFile {
  app: 'synthdesk'
  version: 1
  name: string
  camera: { x: number; y: number; z: number }
  snapGrid: boolean
  graph: GraphData
}

const AUTOSAVE_KEY = 'synthdesk.autosave'
const AUTOSAVE_MS = 2000

const isTauri = (): boolean => '__TAURI_INTERNALS__' in window

// tipagem minima do file system access (chrome); fora dele cai no
// fallback de download/input
interface FsWritable {
  write(data: string): Promise<void>
  close(): Promise<void>
}
interface FsFileHandle {
  name: string
  createWritable(): Promise<FsWritable>
  getFile(): Promise<File>
}
interface FsPickers {
  showSaveFilePicker?(opts: unknown): Promise<FsFileHandle>
  showOpenFilePicker?(opts: unknown): Promise<FsFileHandle[]>
}

const PICKER_TYPES = [
  { description: 'synthdesk project', accept: { 'application/json': ['.synthproj'] } },
]

export class ProjectStore {
  private handle: FsFileHandle | null = null // browser (file system access)
  private tauriPath: string | null = null // tauri (dialog nativo)
  private savedJson = '' // ultimo estado gravado em arquivo
  private lastTick = '' // ultimo snapshot visto pelo autosave
  private name = 'UNTITLED_DESK'

  constructor(
    private graph: Graph,
    private cam: Camera,
    private onApply: () => void,
  ) {}

  private snapshot(): ProjectFile {
    return {
      app: 'synthdesk',
      version: 1,
      name: this.name,
      camera: { x: this.cam.x, y: this.cam.y, z: this.cam.z },
      snapGrid: settings.snapGrid,
      graph: this.graph.serialize(),
    }
  }

  private toJson(): string {
    return JSON.stringify(this.snapshot(), null, 2)
  }

  private setNameFromFile(fileName: string): void {
    const base = fileName.replace(/\.synthproj$/i, '').split(/[\\/]/).pop() ?? 'UNTITLED_DESK'
    this.name = base.toUpperCase().replace(/\s+/g, '_') || 'UNTITLED_DESK'
  }

  private refreshSubject(json?: string): void {
    const dirty = (json ?? this.toJson()) !== this.savedJson
    // sem arquivo ainda = sempre "sujo", mas sem asterisco (nada pra
    // comparar); asterisco so quando existe um salvo de referencia
    setSubject(this.name, this.savedJson !== '' && dirty)
  }

  // aplica um projeto na mesa (load/autosave); descarta componente de
  // tipo desconhecido e cabo orfao em vez de quebrar
  apply(data: ProjectFile): boolean {
    if (!data || data.app !== 'synthdesk' || !data.graph) return false
    const known = (t: string): boolean => {
      try {
        spec(t)
        return true
      } catch {
        return false
      }
    }
    const nodes = (data.graph.nodes ?? []).filter((n) => known(n.type))
    const ids = new Set(nodes.map((n) => n.id))
    const cables = (data.graph.cables ?? []).filter(
      (c) => ids.has(c.from.node) && ids.has(c.to.node),
    )
    this.graph.restore({ ...data.graph, nodes, cables })
    if (data.camera) {
      this.cam.x = data.camera.x
      this.cam.y = data.camera.y
      this.cam.z = data.camera.z
    }
    settings.snapGrid = data.snapGrid ?? true
    this.name = data.name || 'UNTITLED_DESK'
    setSnap(settings.snapGrid)
    setZoom(this.cam.z)
    setNodeCount(this.graph.nodes.length)
    this.refreshSubject()
    this.onApply()
    return true
  }

  async save(saveAs = false): Promise<void> {
    const json = this.toJson()
    if (isTauri()) {
      const { invoke } = await import('@tauri-apps/api/core')
      const path = await invoke<string | null>('save_project', {
        json,
        path: saveAs ? null : this.tauriPath,
        name: `${this.name.toLowerCase()}.synthproj`,
      })
      if (!path) return // cancelado
      this.tauriPath = path
      this.setNameFromFile(path)
    } else {
      const w = window as unknown as FsPickers
      if (w.showSaveFilePicker) {
        if (saveAs || !this.handle) {
          try {
            this.handle = await w.showSaveFilePicker({
              suggestedName: `${this.name.toLowerCase()}.synthproj`,
              types: PICKER_TYPES,
            })
          } catch {
            return // cancelado
          }
        }
        const out = await this.handle.createWritable()
        await out.write(json)
        await out.close()
        this.setNameFromFile(this.handle.name)
      } else {
        // fallback: download classico (sem re-save silencioso)
        const a = document.createElement('a')
        a.href = URL.createObjectURL(new Blob([json], { type: 'application/json' }))
        a.download = `${this.name.toLowerCase()}.synthproj`
        a.click()
        URL.revokeObjectURL(a.href)
      }
    }
    this.savedJson = json
    this.refreshSubject(json)
  }

  async load(): Promise<void> {
    let json: string | null = null
    if (isTauri()) {
      const { invoke } = await import('@tauri-apps/api/core')
      const res = await invoke<{ path: string; json: string } | null>('load_project')
      if (!res) return // cancelado
      json = res.json
      this.tauriPath = res.path
      this.setNameFromFile(res.path)
    } else {
      const w = window as unknown as FsPickers
      if (w.showOpenFilePicker) {
        try {
          const [h] = await w.showOpenFilePicker({ types: PICKER_TYPES })
          this.handle = h
          json = await (await h.getFile()).text()
          this.setNameFromFile(h.name)
        } catch {
          return // cancelado
        }
      } else {
        json = await new Promise<string | null>((resolve) => {
          const inp = document.createElement('input')
          inp.type = 'file'
          inp.accept = '.synthproj,application/json'
          inp.onchange = () => {
            const f = inp.files?.[0]
            if (!f) return resolve(null)
            this.setNameFromFile(f.name)
            void f.text().then(resolve)
          }
          inp.click()
        })
      }
    }
    if (!json) return
    try {
      if (this.apply(JSON.parse(json) as ProjectFile)) {
        this.savedJson = this.toJson()
        this.refreshSubject()
      }
    } catch {
      // arquivo invalido: mesa fica como esta
    }
  }

  // autosave: roda no intervalo; so trabalha quando algo mudou
  private tick(): void {
    const json = this.toJson()
    if (json === this.lastTick) return
    this.lastTick = json
    try {
      localStorage.setItem(AUTOSAVE_KEY, json)
    } catch {
      // storage cheio/indisponivel: segue sem autosave local
    }
    // projeto com arquivo = save automatico de verdade no arquivo
    if (this.tauriPath) {
      void import('@tauri-apps/api/core').then(({ invoke }) =>
        invoke('save_project', { json, path: this.tauriPath, name: '' }).then(() => {
          this.savedJson = json
          this.refreshSubject(json)
        }),
      )
    } else if (this.handle) {
      void this.handle
        .createWritable()
        .then(async (out) => {
          await out.write(json)
          await out.close()
          this.savedJson = json
          this.refreshSubject(json)
        })
        .catch(() => {})
    } else {
      this.refreshSubject(json)
    }
  }

  // boot: recupera a sessao do localStorage e liga autosave + atalhos
  init(): void {
    const saved = localStorage.getItem(AUTOSAVE_KEY)
    if (saved) {
      try {
        this.apply(JSON.parse(saved) as ProjectFile)
      } catch {
        // autosave corrompido: comeca vazio
      }
    }
    setSubject(this.name, false)
    setInterval(() => this.tick(), AUTOSAVE_MS)
    window.addEventListener('beforeunload', () => {
      try {
        localStorage.setItem(AUTOSAVE_KEY, this.toJson())
      } catch {
        // sem storage, sem drama
      }
    })
    window.addEventListener('keydown', (e) => {
      if (!(e.metaKey || e.ctrlKey)) return
      if (e.key === 's' || e.key === 'S') {
        e.preventDefault()
        void this.save(e.shiftKey)
      } else if (e.key === 'o' || e.key === 'O') {
        e.preventDefault()
        void this.load()
      }
    })
  }
}
