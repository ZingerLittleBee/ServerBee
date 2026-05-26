import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { SpaThemeSummary } from '@/api/spa-themes'
import '@/lib/i18n'
import { SpaThemeDetailsDrawer } from './spa-theme-details-drawer'

const DOWNLOAD_RE = /Download package/

const theme: SpaThemeSummary = {
  author: 'Acme Inc.',
  description: 'A pretty theme',
  has_preview: true,
  is_active: false,
  is_superseded: false,
  manifest_id: 'com.acme.theme',
  name: 'Acme',
  size_bytes: 2048,
  uploaded_at: '2026-05-26T10:00:00Z',
  uploaded_by: 'admin',
  uuid: 'aaaa-bbbb',
  version: '1.0.0'
}

describe('SpaThemeDetailsDrawer', () => {
  it('renders summary fields and a download link when a theme is provided', () => {
    render(<SpaThemeDetailsDrawer onClose={vi.fn()} theme={theme} />)

    expect(screen.getByText('com.acme.theme')).toBeInTheDocument()
    expect(screen.getByText('aaaa-bbbb')).toBeInTheDocument()
    expect(screen.getByText('A pretty theme')).toBeInTheDocument()

    const link = screen.getByRole('link', { name: DOWNLOAD_RE }) as HTMLAnchorElement
    expect(link.getAttribute('href')).toBe('/api/settings/spa-themes/aaaa-bbbb/package')
  })
})
