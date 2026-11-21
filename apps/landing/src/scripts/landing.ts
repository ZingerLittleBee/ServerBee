/* The page's whole client runtime: the theme toggle, the install tabs and the copy buttons. */

type Theme = 'dark' | 'light'

const root = document.documentElement
const prefersDark = matchMedia('(prefers-color-scheme: dark)')
const themeColors: Record<Theme, string> = { dark: '#0d0e11', light: '#fbfbfc' }

function storedTheme(): Theme | undefined {
  const theme = root.dataset.theme
  return theme === 'dark' || theme === 'light' ? theme : undefined
}

function systemTheme(): Theme {
  return prefersDark.matches ? 'dark' : 'light'
}

/** The media query that decides whether a capture's dark source applies under the current choice. */
function darkSourceMedia(choice: Theme | undefined): string {
  if (choice === 'dark') {
    return 'all'
  }
  return choice === 'light' ? 'not all' : '(prefers-color-scheme: dark)'
}

/** Captures and the browser chrome color follow the system unless the visitor picked a theme. */
function applyTheme(): void {
  const choice = storedTheme()
  const media = darkSourceMedia(choice)
  for (const source of document.querySelectorAll<HTMLSourceElement>('source[data-dark]')) {
    source.media = media
  }
  for (const meta of document.querySelectorAll<HTMLMetaElement>('meta[data-theme-color]')) {
    meta.content = themeColors[choice ?? (meta.dataset.themeColor as Theme)]
  }
}

function toggleTheme(): void {
  const next: Theme = (storedTheme() ?? systemTheme()) === 'dark' ? 'light' : 'dark'
  // Choosing what the system already shows clears the choice, so the page follows the system again.
  const choice = next === systemTheme() ? undefined : next
  if (choice) {
    root.dataset.theme = choice
  } else {
    delete root.dataset.theme
  }
  try {
    if (choice) {
      localStorage.setItem('theme', choice)
    } else {
      localStorage.removeItem('theme')
    }
  } catch {
    // Storage is unavailable: the choice lasts for this page view.
  }
  applyTheme()
}

class InstallCommand extends HTMLElement {
  connectedCallback(): void {
    const tabs = [...this.querySelectorAll<HTMLButtonElement>('[role="tab"]')]
    const select = (tab: HTMLButtonElement, focus: boolean): void => {
      for (const other of tabs) {
        const selected = other === tab
        other.setAttribute('aria-selected', String(selected))
        other.tabIndex = selected ? 0 : -1
        const panel = document.getElementById(other.getAttribute('aria-controls') ?? '')
        if (panel) {
          panel.hidden = !selected
        }
      }
      if (focus) {
        tab.focus()
      }
    }
    const keys: Record<string, (index: number) => number> = {
      ArrowLeft: (index) => (index - 1 + tabs.length) % tabs.length,
      ArrowRight: (index) => (index + 1) % tabs.length,
      End: () => tabs.length - 1,
      Home: () => 0
    }
    for (const [index, tab] of tabs.entries()) {
      tab.addEventListener('click', () => select(tab, false))
      tab.addEventListener('keydown', (event) => {
        const move = keys[event.key]
        if (move) {
          event.preventDefault()
          select(tabs[move(index)], true)
        }
      })
    }
  }
}

const resetTimers = new WeakMap<HTMLButtonElement, number>()

async function copyCommand(button: HTMLButtonElement): Promise<void> {
  const code = button.closest('.cmd')?.querySelector('code')
  const label = button.querySelector<HTMLElement>('[data-label]')
  const status = document.getElementById('copy-status')
  if (!(code && label && status)) {
    return
  }
  let state: 'copied' | 'failed' = 'copied'
  try {
    await navigator.clipboard.writeText(code.textContent ?? '')
  } catch {
    // No clipboard access: select the command so the visitor can copy it by hand.
    state = 'failed'
    getSelection()?.selectAllChildren(code)
  }
  button.dataset.state = state
  label.textContent = (state === 'copied' ? button.dataset.copied : button.dataset.failed) ?? ''
  status.textContent = (state === 'copied' ? button.dataset.copiedStatus : button.dataset.failedStatus) ?? ''
  clearTimeout(resetTimers.get(button))
  resetTimers.set(
    button,
    window.setTimeout(() => {
      delete button.dataset.state
      label.textContent = button.dataset.idle ?? ''
      status.textContent = ''
    }, 2200)
  )
}

customElements.define('install-command', InstallCommand)

document.addEventListener('click', (event) => {
  if (!(event.target instanceof Element)) {
    return
  }
  if (event.target.closest('[data-theme-toggle]')) {
    toggleTheme()
    return
  }
  const copy = event.target.closest<HTMLButtonElement>('button[data-copy]')
  if (copy) {
    copyCommand(copy)
  }
})

prefersDark.addEventListener('change', applyTheme)
applyTheme()

const header = document.querySelector('.header')
if (header) {
  const markStuck = (): void => {
    header.toggleAttribute('data-stuck', window.scrollY > 4)
  }
  window.addEventListener('scroll', markStuck, { passive: true })
  markStuck()
}
