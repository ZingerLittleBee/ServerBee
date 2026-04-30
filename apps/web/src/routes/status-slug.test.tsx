import { describe, expect, it } from 'vitest'
import { applyStatusPageTheme } from './status.$slug'

describe('applyStatusPageTheme', () => {
  it('injects custom theme variables into the status page root only', () => {
    const root = document.createElement('div')
    root.className = 'status-page-root'

    applyStatusPageTheme(root, {
      id: 7,
      kind: 'custom',
      name: 'Custom',
      updated_at: '2026-04-30T00:00:00Z',
      vars_dark: { background: 'oklch(0 0 0)' },
      vars_light: { background: 'oklch(1 0 0)' }
    })

    const style = root.querySelector('style[data-status-theme]')

    expect(style?.textContent).toContain('.status-page-root {')
    expect(style?.textContent).toContain('--background: oklch(1 0 0);')
    expect(style?.textContent).toContain('.status-page-root.dark {')
    expect(document.documentElement.querySelector('style[data-status-theme]')).toBeNull()
  })
})
