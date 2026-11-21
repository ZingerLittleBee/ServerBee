/*
 * The page's client runtime: the theme toggle, the install tabs and the copy buttons, each with its feedback, and
 * the motion in ./motion. The page reads the same without any of it.
 */

import { mountMotion } from './motion'
import { EASE_OUT, reduced } from './motion/stage'

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

/** Switches the theme. Where the browser can, the new theme grows out of the toggle instead of snapping in. */
function switchTheme(toggle: Element): void {
  if (reduced || !document.startViewTransition) {
    toggleTheme()
    return
  }
  const box = toggle.getBoundingClientRect()
  const x = box.left + box.width / 2
  const y = box.top + box.height / 2
  root.style.setProperty('--theme-x', `${x}px`)
  root.style.setProperty('--theme-y', `${y}px`)
  root.style.setProperty('--theme-r', `${Math.hypot(Math.max(x, innerWidth - x), Math.max(y, innerHeight - y))}px`)
  root.classList.add('theming')
  document.startViewTransition(toggleTheme).finished.finally(() => root.classList.remove('theming'))
}

class InstallCommand extends HTMLElement {
  connectedCallback(): void {
    const tablist = this.querySelector<HTMLElement>('[role="tablist"]')
    const tabs = [...this.querySelectorAll<HTMLButtonElement>('[role="tab"]')]
    // One highlight for the whole row, clipped to the selected tab, so it slides from tab to tab.
    const pill = document.createElement('span')
    pill.className = 'tab-pill'
    pill.setAttribute('aria-hidden', 'true')
    const placePill = (): void => {
      const tab = tabs.find((other) => other.getAttribute('aria-selected') === 'true')
      if (!(tab && tablist)) {
        return
      }
      const inset = pill.offsetLeft
      const left = tab.offsetLeft - inset
      pill.style.setProperty('--left', `${left}px`)
      pill.style.setProperty('--right', `${pill.offsetWidth - left - tab.offsetWidth}px`)
    }
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
      // From here on a panel that appears was switched to, which is what its contents fade in for.
      this.dataset.switched = ''
      placePill()
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
    if (tablist) {
      tablist.prepend(pill)
      tablist.dataset.pill = ''
      placePill()
      addEventListener('resize', placePill)
      document.fonts.ready.then(placePill)
      // The highlight takes its first place without sliding there.
      requestAnimationFrame(() => requestAnimationFrame(() => pill.setAttribute('data-ready', '')))
    }
  }
}

const resetTimers = new WeakMap<HTMLButtonElement, number>()

/**
 * Changes a copy button's label. The button keeps its natural size for whichever label shows: its width eases to
 * the new one instead of jumping, and the label fades in.
 */
function relabel(button: HTMLButtonElement, label: HTMLElement, text: string, state?: 'copied' | 'failed'): void {
  const before = button.getBoundingClientRect().width
  if (state) {
    button.dataset.state = state
  } else {
    delete button.dataset.state
  }
  label.textContent = text
  if (reduced) {
    return
  }
  const after = button.getBoundingClientRect().width
  const timing = { duration: 220, easing: EASE_OUT }
  label.animate([{ opacity: 0 }, { opacity: 1 }], timing)
  // The full-width button on a phone keeps its width, so there is nothing to ease there.
  if (Math.abs(after - before) > 0.5) {
    button.animate([{ width: `${before}px` }, { width: `${after}px` }], timing)
  }
}

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
  relabel(button, label, (state === 'copied' ? button.dataset.copied : button.dataset.failed) ?? '', state)
  status.textContent = (state === 'copied' ? button.dataset.copiedStatus : button.dataset.failedStatus) ?? ''
  clearTimeout(resetTimers.get(button))
  resetTimers.set(
    button,
    window.setTimeout(() => {
      relabel(button, label, button.dataset.idle ?? '')
      status.textContent = ''
    }, 2200)
  )
}

customElements.define('install-command', InstallCommand)

document.addEventListener('click', (event) => {
  if (!(event.target instanceof Element)) {
    return
  }
  const toggle = event.target.closest('[data-theme-toggle]')
  if (toggle) {
    switchTheme(toggle)
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

mountMotion()
