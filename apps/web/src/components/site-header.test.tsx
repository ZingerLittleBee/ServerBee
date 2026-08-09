import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { SiteHeader, SiteHeaderActions, SiteHeaderActionsProvider, SiteHeaderLeading } from './site-header'

vi.mock('@/components/ui/separator', () => ({
  Separator: () => <span aria-hidden="true" />
}))

vi.mock('@/components/ui/sidebar', () => ({
  SidebarTrigger: () => <button type="button">Menu</button>
}))

describe('SiteHeader portals', () => {
  it('renders leading and trailing route content inside the site header', () => {
    render(
      <SiteHeaderActionsProvider>
        <SiteHeader>
          <h1>Servers</h1>
        </SiteHeader>
        <SiteHeaderLeading>
          <span>Leading control</span>
        </SiteHeaderLeading>
        <SiteHeaderActions>
          <button type="button">Add server</button>
        </SiteHeaderActions>
      </SiteHeaderActionsProvider>
    )

    const banner = screen.getByRole('banner')
    expect(banner).toContainElement(screen.getByText('Leading control'))
    expect(banner).toContainElement(screen.getByRole('button', { name: 'Add server' }))
    // Leading sits before the title; trailing after (ml-auto).
    expect(banner.textContent?.indexOf('Leading control') ?? -1).toBeLessThan(
      banner.textContent?.indexOf('Servers') ?? -1
    )
    expect(banner.textContent?.indexOf('Servers') ?? -1).toBeLessThan(banner.textContent?.indexOf('Add server') ?? -1)
  })
})
