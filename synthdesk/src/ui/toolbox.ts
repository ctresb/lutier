import { icon, type IconName } from './icon'

export interface Tool {
  icon: IconName
  name: string
  run(): void
}

// toolbox no canto superior direito, mesma anatomia do components box
export function initToolbox(tools: Tool[]): void {
  const list = document.getElementById('toolbox-list')
  const count = document.getElementById('toolbox-count')
  if (!list || !count) throw new Error('toolbox ausente no html')
  count.textContent = `${String(tools.length).padStart(2, '0')} TOOLS`

  tools.forEach((t, i) => {
    const btn = document.createElement('button')
    btn.type = 'button'
    btn.className = 'tool-item'
    btn.setAttribute('role', 'listitem')
    btn.setAttribute('aria-label', `run ${t.name}`)
    const idx = document.createElement('span')
    idx.className = 'comp-idx'
    idx.textContent = String(i + 1).padStart(2, '0')
    const ic = icon(t.icon)
    const name = document.createElement('span')
    name.className = 'tool-name'
    name.textContent = t.name
    btn.append(idx, ic, name)
    btn.addEventListener('click', () => t.run())
    list.appendChild(btn)
  })
}
