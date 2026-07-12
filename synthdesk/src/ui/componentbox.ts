import type { Input } from '../core/input'
import { COMPONENTS } from '../components/registry'
import { setStatus } from './hud'

// popula o components box e implementa drag pro desk + colocar por teclado
export function initComponentBox(input: Input): void {
  const list = document.getElementById('componentbox-list')
  const count = document.getElementById('componentbox-count')
  if (!list || !count) throw new Error('components box ausente no html')
  count.textContent = `${String(COMPONENTS.length).padStart(2, '0')} TYPES`

  let lastCat = ''
  COMPONENTS.forEach((m, i) => {
    // header de categoria: label faint entre os grupos
    if (m.category !== lastCat) {
      lastCat = m.category
      const cat = document.createElement('div')
      cat.className = 'comp-cat'
      cat.textContent = m.category
      list.appendChild(cat)
    }
    const btn = document.createElement('button')
    btn.type = 'button'
    btn.className = 'comp-item'
    btn.setAttribute('role', 'listitem')
    btn.setAttribute('aria-label', `place ${m.title} component`)
    for (const [cls, txt] of [
      ['comp-idx', String(i + 1).padStart(2, '0')],
      ['comp-name', m.title],
      ['comp-tag', m.tag],
    ]) {
      const span = document.createElement('span')
      span.className = cls
      span.textContent = txt
      btn.appendChild(span)
    }

    // teclado: enter/espaco coloca no centro da vista
    btn.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault()
        input.placeAtCenter(m.type)
      }
    })

    // pointer: clique coloca no centro, arrasto solta onde o cursor largar
    btn.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return
      e.preventDefault()
      const startX = e.clientX
      const startY = e.clientY
      let ghost: HTMLElement | null = null

      const onMove = (ev: PointerEvent): void => {
        if (!ghost && Math.hypot(ev.clientX - startX, ev.clientY - startY) > 4) {
          ghost = document.createElement('div')
          ghost.id = 'comp-ghost'
          ghost.textContent = m.title
          document.body.appendChild(ghost)
          btn.classList.add('dragging')
          setStatus('PLACING')
        }
        if (ghost) {
          ghost.style.left = `${ev.clientX}px`
          ghost.style.top = `${ev.clientY}px`
        }
      }
      const onUp = (ev: PointerEvent): void => {
        window.removeEventListener('pointermove', onMove)
        window.removeEventListener('pointerup', onUp)
        btn.classList.remove('dragging')
        if (ghost) {
          ghost.remove()
          // solta no desk so se largou fora do proprio components box
          const el = document.elementFromPoint(ev.clientX, ev.clientY)
          if (el && el.id === 'desk') input.placeAt(m.type, ev.clientX, ev.clientY)
          setStatus('IDLE')
        } else {
          input.placeAtCenter(m.type)
        }
      }
      window.addEventListener('pointermove', onMove)
      window.addEventListener('pointerup', onUp)
    })

    list.appendChild(btn)
  })
}
