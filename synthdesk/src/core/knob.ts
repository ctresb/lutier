// primitivo de interacao de knob: TODO knob da mesa se comporta
// igual, sempre por aqui. arrasto vertical, acumulacao INCREMENTAL:
// cada movimento aplica so o delta desde o ultimo evento, com a
// sensibilidade daquele instante. assim, segurar/soltar shift no
// meio do gesto muda a precisao SEM pular o valor.

export const KNOB_SENS = 0.006 // curso completo em ~165px de arrasto
export const KNOB_FINE = 0.12 // multiplicador com shift (knob sem mapa)
const PX_FINE = 4 // px de arrasto por passo fino num knob mapeado

// knob com dominio de exibicao (ex: freq do oscillator em hz): o
// gesto anda em MULTIPLOS EXATOS do passo. sem shift so a unidade
// muda (o decimo fica como esta); com shift anda de decimo em decimo
export interface KnobMap {
  to(v: number): number // 0..1 -> dominio de exibicao
  from(d: number): number // dominio de exibicao -> 0..1
  step(fine: boolean): number // passo no dominio de exibicao
}

const clamp01 = (v: number): number => Math.min(1, Math.max(0, v))
// mata residuo de float pra exibicao bater com o passo (0.1 -> 1 casa)
const snap = (d: number): number => Math.round(d * 10) / 10

export class KnobGesture {
  private value: number // 0..1 acumulado continuo
  private lastY: number
  private disp = 0 // valor no dominio de exibicao, na grade do passo
  private acc = 0 // px acumulados no modo fino

  constructor(
    startValue: number,
    startY: number,
    private map: KnobMap | null = null,
  ) {
    this.value = startValue
    this.lastY = startY
    if (map) this.disp = snap(map.to(startValue))
  }

  move(y: number, fine: boolean): number {
    const dyPx = this.lastY - y
    this.lastY = y

    if (!this.map) {
      this.value = clamp01(this.value + dyPx * KNOB_SENS * (fine ? KNOB_FINE : 1))
      return this.value
    }

    const lo = this.map.to(0)
    const hi = this.map.to(1)
    if (fine) {
      // fino: passo fixo por px, preciso em qualquer regiao da curva
      this.acc += dyPx
      const k = Math.trunc(this.acc / PX_FINE)
      if (k !== 0) {
        this.acc -= k * PX_FINE
        this.disp = snap(Math.min(hi, Math.max(lo, this.disp + k * this.map.step(true))))
      }
      this.value = clamp01(this.map.from(this.disp))
    } else {
      // grosso: segue a curva do knob, mas so em passos inteiros -
      // o que o passo nao alcanca (o decimo) fica intocado
      this.acc = 0
      this.value = clamp01(this.value + dyPx * KNOB_SENS)
      const step = this.map.step(false)
      const k = Math.round((this.map.to(this.value) - this.disp) / step)
      if (k !== 0) this.disp = snap(Math.min(hi, Math.max(lo, this.disp + k * step)))
    }
    return clamp01(this.map.from(this.disp))
  }
}
