// parser minimo de glb (gltf 2.0 binario): so POSITION + indices,
// vira lista de arestas unicas normalizada num cubo unitario.
// suficiente pro wireframe; nada de material, textura ou skin.

export interface WireMesh {
  verts: Float32Array          // xyz normalizado, centroide na origem
  edges: Uint32Array           // pares de indices (a, b)
}

interface GltfJson {
  meshes?: { primitives: { attributes: Record<string, number>, indices?: number }[] }[]
  accessors: {
    bufferView?: number, byteOffset?: number, componentType: number,
    count: number, type: string,
  }[]
  bufferViews: { buffer: number, byteOffset?: number, byteLength: number, byteStride?: number }[]
}

export function parseGlb(buf: ArrayBuffer): WireMesh {
  const dv = new DataView(buf)
  if (dv.getUint32(0, true) !== 0x46546c67) throw new Error('glb: magic invalido')
  let off = 12
  let json: GltfJson | null = null
  let bin: ArrayBuffer | null = null
  while (off < dv.byteLength) {
    const len = dv.getUint32(off, true)
    const type = dv.getUint32(off + 4, true)
    const chunk = buf.slice(off + 8, off + 8 + len)
    if (type === 0x4e4f534a) json = JSON.parse(new TextDecoder().decode(chunk))
    else if (type === 0x004e4942) bin = chunk
    off += 8 + len
  }
  if (!json || !bin) throw new Error('glb: chunks faltando')

  const positions: number[] = []
  const edgeSet = new Set<number>()
  let base = 0

  const readIndices = (ai: number): Uint32Array => {
    const acc = json!.accessors[ai]
    const bv = json!.bufferViews[acc.bufferView ?? 0]
    const start = (bv.byteOffset ?? 0) + (acc.byteOffset ?? 0)
    if (acc.componentType === 5123) {
      const a = new Uint16Array(bin!, start, acc.count)
      return Uint32Array.from(a)
    }
    if (acc.componentType === 5125) return new Uint32Array(bin!.slice(start, start + acc.count * 4))
    return Uint32Array.from(new Uint8Array(bin!, start, acc.count))
  }

  for (const mesh of json.meshes ?? []) {
    for (const prim of mesh.primitives) {
      const acc = json.accessors[prim.attributes['POSITION']]
      const bv = json.bufferViews[acc.bufferView ?? 0]
      const start = (bv.byteOffset ?? 0) + (acc.byteOffset ?? 0)
      const stride = (bv.byteStride ?? 12) / 4
      const raw = new Float32Array(bin, start, acc.count * stride - (stride - 3))
      for (let i = 0; i < acc.count; i++) {
        positions.push(raw[i * stride], raw[i * stride + 1], raw[i * stride + 2])
      }
      if (prim.indices === undefined) continue
      const idx = readIndices(prim.indices)
      for (let i = 0; i + 2 < idx.length; i += 3) {
        const a = base + idx[i]
        const b = base + idx[i + 1]
        const c = base + idx[i + 2]
        for (const [p, q] of [[a, b], [b, c], [c, a]]) {
          const lo = Math.min(p, q)
          const hi = Math.max(p, q)
          edgeSet.add(lo * 0x100000 + hi)
        }
      }
      base += acc.count
    }
  }

  const verts = new Float32Array(positions)
  // normaliza: centroide na origem, maior dimensao = 1
  const min = [Infinity, Infinity, Infinity]
  const max = [-Infinity, -Infinity, -Infinity]
  for (let i = 0; i < verts.length; i += 3) {
    for (let k = 0; k < 3; k++) {
      min[k] = Math.min(min[k], verts[i + k])
      max[k] = Math.max(max[k], verts[i + k])
    }
  }
  const cx = (min[0] + max[0]) / 2
  const cy = (min[1] + max[1]) / 2
  const cz = (min[2] + max[2]) / 2
  const scale = 1 / Math.max(max[0] - min[0], max[1] - min[1], max[2] - min[2], 1e-9)
  for (let i = 0; i < verts.length; i += 3) {
    verts[i] = (verts[i] - cx) * scale
    verts[i + 1] = (verts[i + 1] - cy) * scale
    verts[i + 2] = (verts[i + 2] - cz) * scale
  }

  const edges = new Uint32Array(edgeSet.size * 2)
  let j = 0
  for (const key of edgeSet) {
    edges[j++] = Math.floor(key / 0x100000)
    edges[j++] = key % 0x100000
  }
  return { verts, edges }
}
