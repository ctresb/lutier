import type { Vec2 } from './types'

const Z_MIN = 0.15
const Z_MAX = 4.0

// camera 2d: pan em coordenadas de mundo + zoom ancorado no cursor
export class Camera {
  x = 0
  y = 0
  z = 1
  vw = 1
  vh = 1

  toWorld(sx: number, sy: number): Vec2 {
    return {
      x: (sx - this.vw / 2) / this.z + this.x,
      y: (sy - this.vh / 2) / this.z + this.y,
    }
  }

  toScreen(wx: number, wy: number): Vec2 {
    return {
      x: (wx - this.x) * this.z + this.vw / 2,
      y: (wy - this.y) * this.z + this.vh / 2,
    }
  }

  panScreen(dx: number, dy: number): void {
    this.x -= dx / this.z
    this.y -= dy / this.z
  }

  // zoom multiplicativo mantendo o ponto do mundo sob (sx, sy) parado
  zoomAt(sx: number, sy: number, factor: number): void {
    const before = this.toWorld(sx, sy)
    this.z = Math.min(Z_MAX, Math.max(Z_MIN, this.z * factor))
    const after = this.toWorld(sx, sy)
    this.x += before.x - after.x
    this.y += before.y - after.y
  }

  reset(): void {
    this.x = 0
    this.y = 0
    this.z = 1
  }
}
