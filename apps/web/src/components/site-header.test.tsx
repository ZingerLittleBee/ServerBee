import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { SiteHeader, SiteHeaderActions, SiteHeaderActionsProvider } from './site-header'

vi.mock('@/components/ui/separator', () => ({
  Separator: () => <span aria-hidden="true" />
}))

vi.mock('@/components/ui/sidebar', () => ({
  SidebarTrigger: () => <button type="button">Menu</button>
}))

describe('SiteHeaderActions', () => {
  it('renders route actions inside the site header', () => {
    render(
      <SiteHeaderActionsProvider>
        <SiteHeader>
          <h1>Servers</h1>
        </SiteHeader>
        <SiteHeaderActions>
          <button type="button">Add server</button>
        </SiteHeaderActions>
      </SiteHeaderActionsProvider>
    )

    expect(screen.getByRole('banner')).toContainElement(screen.getByRole('button', { name: 'Add server' }))
  })
})
