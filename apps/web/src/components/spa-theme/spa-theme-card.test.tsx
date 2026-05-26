import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { SpaThemeSummary } from '@/api/spa-themes'
import '@/lib/i18n'
import { SpaThemeCard } from './spa-theme-card'

const base: SpaThemeSummary = {
  author: 'Inc',
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

const noop = () => {
  // intentionally empty
}

const ACTIVATE_RE = /Activate/
const DELETE_RE = /Delete/

describe('SpaThemeCard', () => {
  it('shows activate when not active', () => {
    render(
      <SpaThemeCard
        onActivate={noop}
        onDeactivate={noop}
        onDelete={noop}
        onOpenDetails={noop}
        onPreview={noop}
        theme={base}
      />
    )
    expect(screen.getByText(ACTIVATE_RE)).toBeInTheDocument()
  })

  it('disables delete when active', () => {
    const onDelete = vi.fn()
    render(
      <SpaThemeCard
        onActivate={noop}
        onDeactivate={noop}
        onDelete={onDelete}
        onOpenDetails={noop}
        onPreview={noop}
        theme={{ ...base, is_active: true }}
      />
    )
    const btn = screen.getByText(DELETE_RE) as HTMLButtonElement
    expect(btn).toBeDisabled()
    fireEvent.click(btn)
    expect(onDelete).not.toHaveBeenCalled()
  })
})
