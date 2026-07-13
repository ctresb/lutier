import { spec } from '../components/registry'
import type { Graph } from './graph'
import type { NodeState } from './types'

// variaveis globais da mesa: TODO componente expoe cada param como
// NOME_PARAM (OSC_01_ACTIVE, OSC_01_WAVE, OSC_01_FREQ, DEV_01_NOTE,
// RVB_01_WET...). e a superficie de automacao: qualquer coisa que
// leia/escreva aqui controla a mesa - o valor cru e o mesmo param do
// componente (o audio reconcilia no proximo frame sujo).

export interface DeskVar {
  name: string
  node: number // id do componente dono
  param: string
  value: number // valor cru do param
  display: string // valor formatado (TRUE/FALSE, SQUARE, C4, 426.3...)
}

function varName(n: NodeState, param: string): string {
  return `${n.name}_${param === 'on' ? 'ACTIVE' : param.toUpperCase()}`
}

function isSelector(type: string, param: string): boolean {
  return spec(type).controls.some((c) => c.kind === 'selector' && c.id === param)
}

function isSlider(type: string, param: string): boolean {
  return spec(type).controls.some((c) => c.kind === 'slider' && c.param === param)
}

function displayOf(n: NodeState, param: string, value: number): string {
  const s = spec(n.type)
  if (param === 'on') return value > 0 ? 'TRUE' : 'FALSE'
  if (s.selectorValue && isSelector(n.type, param)) return s.selectorValue(n, param)
  if (s.sliderValue && isSlider(n.type, param)) return s.sliderValue(n, param)
  return value.toFixed(3)
}

export function listVars(graph: Graph): DeskVar[] {
  const out: DeskVar[] = []
  for (const n of graph.nodes) {
    for (const [param, value] of Object.entries(n.params)) {
      out.push({ name: varName(n, param), node: n.id, param, value, display: displayOf(n, param, value) })
    }
  }
  return out
}

export function getVar(graph: Graph, name: string): DeskVar | null {
  return listVars(graph).find((v) => v.name === name) ?? null
}

// escreve numa variavel: booleano/TRUE/FALSE para ACTIVE e toggles,
// rotulo (SQUARE, HALL...) para selectors, numero cru pro resto.
// retorna false se a variavel nao existe ou o valor nao casa.
export function setVar(graph: Graph, name: string, value: number | string | boolean): boolean {
  for (const n of graph.nodes) {
    for (const param of Object.keys(n.params)) {
      if (varName(n, param) !== name) continue
      if (param === 'on' || typeof value === 'boolean' || value === 'TRUE' || value === 'FALSE') {
        n.params[param] = value === true || value === 'TRUE' || value === 1 ? 1 : 0
        return true
      }
      if (typeof value === 'string' && isSelector(n.type, param)) {
        // seta por rotulo: prova indices ate o selectorValue bater
        const s = spec(n.type)
        const keep = n.params[param]
        for (let i = 0; i < 32; i++) {
          n.params[param] = i
          if (s.selectorValue?.(n, param).toUpperCase() === value.toUpperCase()) return true
        }
        n.params[param] = keep
        return false
      }
      if (typeof value === 'number' && Number.isFinite(value)) {
        n.params[param] = value
        return true
      }
      return false
    }
  }
  return false
}
