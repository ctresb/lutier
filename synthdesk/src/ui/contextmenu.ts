import type { Camera } from '../core/camera'
import type { Graph } from '../core/graph'
import type { Input } from '../core/input'
import { settings } from '../core/settings'
import type { Renderer } from '../render/renderer'
import { icon, type IconName } from './icon'

interface MenuItem {
  icon: IconName
  label: string
  disabled?: boolean
  action(): void
}

// menu de contexto no estilo lumiere (painel com brackets)
export function initContextMenu(
  canvas: HTMLCanvasElement,
  cam: Camera,
  graph: Graph,
  renderer: Renderer,
  input: Input,
): void {
  let menu: HTMLElement | null = null

  const close = (): void => {
    menu?.remove()
    menu = null
  }

  const open = (x: number, y: number, items: MenuItem[]): void => {
    close()
    menu = document.createElement('div')
    menu.className = 'panel ctx-menu'
    menu.setAttribute('role', 'menu')
    for (const cls of ['bk bk-tl', 'bk bk-tr', 'bk bk-bl', 'bk bk-br']) {
      const i = document.createElement('i')
      i.className = cls
      menu.appendChild(i)
    }
    for (const it of items) {
      const btn = document.createElement('button')
      btn.type = 'button'
      btn.className = 'ctx-item'
      btn.disabled = it.disabled ?? false
      btn.setAttribute('role', 'menuitem')
      const ic = icon(it.icon)
      ic.classList.add('ctx-icon')
      const lb = document.createElement('span')
      lb.textContent = it.label
      btn.append(ic, lb)
      btn.addEventListener('click', () => {
        it.action()
        close()
      })
      menu.appendChild(btn)
    }
    document.body.appendChild(menu)
    // clampa dentro da viewport
    const r = menu.getBoundingClientRect()
    menu.style.left = `${Math.min(x, window.innerWidth - r.width - 8)}px`
    menu.style.top = `${Math.min(y, window.innerHeight - r.height - 8)}px`
    menu.querySelector('button:not([disabled])')?.setAttribute('autofocus', '')
  }

  canvas.addEventListener('contextmenu', (e) => {
    e.preventDefault()
    const w = cam.toWorld(e.clientX, e.clientY)
    const hit = graph.hitTest(w.x, w.y, cam.z)

    if (hit && (hit.t === 'body' || hit.t === 'knob' || hit.t === 'port')) {
      const n = hit.node
      renderer.selectOnly(n.id)
      renderer.selectedCable = null
      renderer.invalidate()
      // lock/unlock vive no toggle do header, nao aqui
      open(e.clientX, e.clientY, [
        {
          icon: 'trash',
          label: 'DELETE COMPONENT',
          disabled: n.locked ?? false,
          action: () => {
            graph.removeNode(n.id)
            renderer.clearSelection()
            renderer.invalidate()
          },
        },
      ])
    } else if (hit && hit.t === 'waypoint') {
      // botao direito num angulo remove o ponto na hora, sem menu
      hit.cable.pts.splice(hit.index, 1)
      renderer.invalidate()
    } else if (hit && hit.t === 'cable') {
      renderer.selectedCable = hit.cable.id
      renderer.clearSelection()
      renderer.invalidate()
      const cable = hit.cable
      open(e.clientX, e.clientY, [
        {
          icon: 'trash',
          label: 'DELETE CABLE',
          action: () => {
            graph.removeCable(cable.id)
            renderer.selectedCable = null
            renderer.invalidate()
          },
        },
      ])
    } else {
      open(e.clientX, e.clientY, [
        {
          icon: settings.snapGrid ? 'grid-disable' : 'grid-enable',
          label: settings.snapGrid ? 'DISABLE GRID SNAP' : 'ENABLE GRID SNAP',
          action: () => input.toggleSnap(),
        },
      ])
    }
  })

  window.addEventListener('pointerdown', (e) => {
    if (menu && !menu.contains(e.target as Node)) close()
  })
  window.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') close()
  })
  window.addEventListener('blur', close)
}
