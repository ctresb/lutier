import type { Graph } from '../core/graph'
import type { KnobMap } from '../core/knob'
import type { NodeState, ParamSpec, PortKind, PortSpec } from '../core/types'

// unidade de medida do synthdesk: 1u = um passo da grade de pontos.
// todo componente mede N x M unidades INTEIRAS, entao os quatro
// cantos caem exatamente nas bolinhas quando o snap ta ligado.
export const UNIT = 46
// header empilhado: nome em cima, tag embaixo, toggle de lock a direita
export const HEADER_H = 42
// faixa inferior obrigatoria de inputs/outputs (componente nodePort)
export const IO_H = 36

export interface DrawOpts {
  zoom: number
  selected: boolean
  hoverKnob: string | null // param do knob sob o cursor
  hoverSlider: string | null // param do slider sob o cursor
  hoverPort: PortSpec | null
  // cv chegando num input deste node (null = nada plugado/fonte off)
  cvInto(port: string): number | null
}

export interface IoSpec {
  id: string
  kind: PortKind
  label: string
}

// ---------------------------------------------------------------
// a BASE de um componente e DECLARATIVA (json puro): tamanho, io,
// params e controles. o que e especifico vira hook opcional
// (desenho extra, acao de selector, cv). nada de layout hardcoded
// dentro do componente: os controles padrao sao renderizados,
// clicados e arrastados pela base.
//
// TODO componente tem energia: a base injeta o param 'on' (def 1) e
// desenha o switch de ON/OFF no header, a esquerda do nome.
// desligado, o componente fica inerte: cv nao sai, som nao sai.
// ---------------------------------------------------------------

export type ControlSpec =
  // knob rotativo 270 graus; arrasto vertical edita o param 0..1
  | { kind: 'knob'; param: string; x: number; y: number; r: number }
  // slider horizontal: label 9px faint + valor a direita na linha de
  // cima, trilho em y+18 (respiro pro handle nao invadir o texto),
  // handle quadrado 7x7 (mesma linguagem do waypoint); clique/arrasto
  // posiciona absoluto, 0 na esquerda. steps = N posicoes com detent
  // (ex: 6 -> 0, .2, .4, .6, .8, 1); valor exibido customiza no hook
  // sliderValue do spec
  | { kind: 'slider'; param: string; x: number; y: number; w: number; label: string; steps?: number }
  // toggle: quadrado 10x10 com quadradinho dentro (bool)
  | { kind: 'toggle'; param: string; x: number; y: number; label: string }
  // switch: retangulo com cursor que desliza (esq = off escuro,
  // dir = on claro) (bool)
  | { kind: 'switch'; param: string; x: number; y: number; label: string }
  // linha chave/valor; clique circula (acao no hook press do spec,
  // valor vem do hook selectorValue)
  | { kind: 'selector'; id: string; x: number; y: number; label: string }
  // leitura numerica do param (v.toFixed(3)), centrada em x
  | { kind: 'readout'; param: string; x: number; y: number }
  // texto estatico 9px faint centrado
  | { kind: 'label'; text: string; x: number; y: number }
  // regua horizontal de margem a margem
  | { kind: 'rule'; y: number }

export interface ComponentSpec {
  type: string
  title: string // nome no components box
  tag: string // ex 'CV SRC'
  // grupo no components box (PRIMITIVES / CONTROLLERS / OPERATORS /
  // GENERATORS / EFFECTS); a ordem do registry agrupa
  category: string
  prefix: string // prefixo da instancia, ex 'POT'
  unitsW: number // largura em unidades (inteiro)
  unitsH: number // altura em unidades (inteiro)
  inputs: IoSpec[]
  outputs: IoSpec[]
  params: ParamSpec[]
  controls: ControlSpec[]
  // hooks especificos do componente (tudo opcional):
  drawExtra?(g: CanvasRenderingContext2D, node: NodeState, o: DrawOpts): void
  press?(node: NodeState, button: string): void // selectors
  selectorValue?(node: NodeState, id: string): string
  // valor exibido de um slider (padrao: .3f); ex walls -> '05'
  sliderValue?(node: NodeState, param: string): string
  cvOut?(node: NodeState, port: string, graph: Graph, seen: Set<number>): number
  animates?(node: NodeState, graph?: Graph): boolean // true = redesenha todo frame
  // knob com dominio de exibicao e passos (ex: freq em hz inteiro,
  // decimo so com shift); sem o hook o knob e continuo 0..1
  knobMap?(param: string): KnobMap | null
}

export function sizeOf(s: ComponentSpec): { w: number; h: number } {
  return { w: s.unitsW * UNIT, h: s.unitsH * UNIT }
}

// posicoes padronizadas dos ports: sempre na faixa inferior,
// inputs da esquerda pra direita, outputs da direita pra esquerda
export function portsOf(s: ComponentSpec): PortSpec[] {
  const { w, h } = sizeOf(s)
  const y = h - 15 // centro do quadrado do nodePort (linha + 21)
  const step = 42
  const ports: PortSpec[] = []
  s.inputs.forEach((p, i) => {
    ports.push({ id: p.id, dir: 'in', kind: p.kind, label: p.label, x: 18 + i * step, y })
  })
  s.outputs.forEach((p, i) => {
    ports.push({ id: p.id, dir: 'out', kind: p.kind, label: p.label, x: w - 18 - i * step, y })
  })
  return ports
}

// zonas derivadas dos controles: a base cuida de hit e interacao
export interface KnobZone {
  param: string
  cx: number
  cy: number
  r: number
}

export interface ButtonZone {
  id: string
  ctrl: 'toggle' | 'switch' | 'selector'
  param?: string
  x: number
  y: number
  w: number
  h: number
}

export function knobsOf(s: ComponentSpec): KnobZone[] {
  const out: KnobZone[] = []
  for (const c of s.controls) {
    if (c.kind === 'knob') out.push({ param: c.param, cx: c.x, cy: c.y, r: c.r })
  }
  return out
}

// zona de slider: retangulo em volta do trilho (que fica em y+18)
export interface SliderZone {
  param: string
  x: number
  y: number // y do TRILHO
  w: number
  steps?: number | undefined
}

export function slidersOf(s: ComponentSpec): SliderZone[] {
  const out: SliderZone[] = []
  for (const c of s.controls) {
    if (c.kind === 'slider') {
      out.push({ param: c.param, x: c.x, y: c.y + 18, w: c.w, steps: c.steps })
    }
  }
  return out
}

export function buttonsOf(s: ComponentSpec): ButtonZone[] {
  const { w } = sizeOf(s)
  const out: ButtonZone[] = []
  for (const c of s.controls) {
    if (c.kind === 'toggle' && c.label === '') {
      // toggle SEM label (celula, ex: passo do sequencer): zona
      // compacta so no quadrado, permite varios na mesma linha
      out.push({ id: c.param, ctrl: c.kind, param: c.param, x: c.x - 3, y: c.y - 3, w: 16, h: 16 })
    } else if (c.kind === 'toggle' || c.kind === 'switch') {
      out.push({ id: c.param, ctrl: c.kind, param: c.param, x: c.x - 2, y: c.y - 3, w: w - 12 - c.x + 2, h: 17 })
    } else if (c.kind === 'selector') {
      out.push({ id: c.id, ctrl: 'selector', x: c.x - 2, y: c.y - 4, w: w - 12 - c.x + 2, h: 16 })
    }
  }
  return out
}
