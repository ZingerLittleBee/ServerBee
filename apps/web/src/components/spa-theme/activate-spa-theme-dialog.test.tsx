import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { SpaThemeSummary } from '@/api/spa-themes'
import '@/lib/i18n'
import { ActivateSpaThemeDialog } from './activate-spa-theme-dialog'

const ACTIVATE_RE = /^Activate$/
const CHECKBOX_RE = /I understand/

const theme: SpaThemeSummary = {
  author: 'Acme Inc.',
  description: null,
  has_preview: false,
  is_active: false,
  is_superseded: false,
  manifest_id: 'm',
  name: 'Acme',
  size_bytes: 1,
  uploaded_at: '2026-05-26',
  uploaded_by: 'u',
  uuid: 'u1',
  version: '1.0.0'
}

describe('ActivateSpaThemeDialog', () => {
  it('disables confirm until the checkbox is ticked', () => {
    const onConfirm = vi.fn()
    render(<ActivateSpaThemeDialog onConfirm={onConfirm} onOpenChange={vi.fn()} open theme={theme} />)

    const confirmBtn = screen.getByRole('button', { name: ACTIVATE_RE }) as HTMLButtonElement
    expect(confirmBtn).toBeDisabled()

    // The Base UI Checkbox renders inside a <label>; clicking the label text toggles it.
    fireEvent.click(screen.getByText(CHECKBOX_RE))

    expect(confirmBtn).toBeEnabled()
    fireEvent.click(confirmBtn)
    expect(onConfirm).toHaveBeenCalledTimes(1)
  })

  it('renders nothing when theme is null', () => {
    const { container } = render(
      <ActivateSpaThemeDialog onConfirm={vi.fn()} onOpenChange={vi.fn()} open theme={null} />
    )
    expect(container).toBeEmptyDOMElement()
  })
})
