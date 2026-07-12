// modal de confirmacao no estilo lumiere (painel com brackets).
// enter confirma, esc/clique fora cancela. um por vez.
let openNow = false

export function confirmDelete(count: number): Promise<boolean> {
  if (openNow) return Promise.resolve(false)
  openNow = true

  return new Promise((resolve) => {
    const backdrop = document.createElement('div')
    backdrop.className = 'modal-backdrop'
    const panel = document.createElement('div')
    panel.className = 'panel modal'
    panel.setAttribute('role', 'alertdialog')
    panel.setAttribute('aria-modal', 'true')
    for (const cls of ['bk bk-tl', 'bk bk-tr', 'bk bk-bl', 'bk bk-br']) {
      const i = document.createElement('i')
      i.className = cls
      panel.appendChild(i)
    }
    const title = document.createElement('div')
    title.className = 'panel-title'
    title.textContent = 'DELETE'
    const rule = document.createElement('div')
    rule.className = 'panel-rule'
    const msg = document.createElement('div')
    msg.className = 'modal-msg'
    msg.textContent =
      count === 1 ? 'REMOVE 1 COMPONENT FROM THE DESK?' : `REMOVE ${count} COMPONENTS FROM THE DESK?`
    const actions = document.createElement('div')
    actions.className = 'modal-actions'
    const mkBtn = (label: string, primary: boolean): HTMLButtonElement => {
      const b = document.createElement('button')
      b.type = 'button'
      b.className = primary ? 'modal-btn modal-btn-primary' : 'modal-btn'
      b.textContent = label
      return b
    }
    const cancel = mkBtn('CANCEL', false)
    const del = mkBtn('DELETE', true)
    actions.append(cancel, del)
    panel.append(title, rule, msg, actions)
    backdrop.appendChild(panel)
    document.body.appendChild(backdrop)
    del.focus()

    const done = (ok: boolean): void => {
      window.removeEventListener('keydown', onKey, true)
      backdrop.remove()
      openNow = false
      resolve(ok)
    }
    const onKey = (e: KeyboardEvent): void => {
      // captura: o resto do app nao ve teclado com o modal aberto
      e.stopPropagation()
      if (e.key === 'Enter') done(true)
      else if (e.key === 'Escape') done(false)
    }
    window.addEventListener('keydown', onKey, true)
    cancel.addEventListener('click', () => done(false))
    del.addEventListener('click', () => done(true))
    backdrop.addEventListener('pointerdown', (e) => {
      if (e.target === backdrop) done(false)
    })
  })
}
