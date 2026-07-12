import { setEngine } from '../ui/hud'

// ponte com o backend tauri (lutier); no browser puro vira standalone
interface EngineInfo {
  name: string
  version: string
}

export async function linkEngine(): Promise<void> {
  const w = window as { __TAURI_INTERNALS__?: unknown }
  if (!w.__TAURI_INTERNALS__) {
    setEngine('STANDALONE')
    return
  }
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const info = await invoke<EngineInfo>('engine_info')
    setEngine(`${info.name.toUpperCase()} ${info.version}`)
  } catch {
    setEngine('OFFLINE')
  }
}
