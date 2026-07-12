export interface Vec2 {
  x: number
  y: number
}

export type PortKind = 'cv' | 'audio'
export type PortDir = 'in' | 'out'

export interface PortSpec {
  id: string
  dir: PortDir
  kind: PortKind
  label: string
  // posicao relativa ao canto superior esquerdo do modulo, em unidades de mundo
  x: number
  y: number
}

export interface ParamSpec {
  id: string
  label: string
  def: number
}

export interface KnobSpec {
  param: string
  cx: number
  cy: number
  r: number
}

export interface SliderSpec {
  param: string
  x: number
  y: number // y do trilho
  w: number
  steps?: number | undefined // N posicoes com detent
}

export interface NodeState {
  id: number
  type: string
  name: string
  x: number
  y: number
  params: Record<string, number>
  locked?: boolean
}

export interface PortRef {
  node: number
  port: string
}

export interface Cable {
  id: number
  from: PortRef // sempre um out
  to: PortRef // sempre um in
  // pontos de roteamento em mundo, na ordem from -> to
  pts: Vec2[]
}

export type Hit =
  | { t: 'knob'; node: NodeState; knob: KnobSpec }
  | { t: 'slider'; node: NodeState; slider: SliderSpec }
  | { t: 'lock'; node: NodeState }
  | { t: 'power'; node: NodeState }
  | { t: 'button'; node: NodeState; button: string }
  | { t: 'port'; node: NodeState; port: PortSpec }
  | { t: 'body'; node: NodeState }
  | { t: 'waypoint'; cable: Cable; index: number }
  | { t: 'cable'; cable: Cable; seg: number; at: Vec2 }
