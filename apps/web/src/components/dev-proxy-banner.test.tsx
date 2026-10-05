import { fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { DevProxyBanner } from './dev-proxy-banner'
import { getDevProxyBannerState } from './dev-proxy-banner-state'

describe('DevProxyBanner', () => {
  afterEach(() => {
    vi.unstubAllEnvs()
  })

  it('shows the read-only warning when writes are disabled', () => {
    const state = getDevProxyBannerState({
      allowWrites: '0',
      mode: 'prod-proxy',
      target: 'https://prod.example.com'
    })

    expect(state?.message).toContain('read-only')
    expect(state?.className).toContain('bg-orange-500')
  })

  it('does not claim read-only when writes are enabled', () => {
    const state = getDevProxyBannerState({
      allowWrites: '1',
      mode: 'prod-proxy',
      target: 'https://prod.example.com'
    })

    expect(state?.message).not.toContain('read-only')
    expect(state?.message).toContain('WRITE ACCESS ENABLED')
    expect(state?.className).toContain('bg-red-700')
  })

  it.each(['0', '1'])('can dismiss the warning with allowWrites=%s until the page reloads', (allowWrites) => {
    vi.stubEnv('MODE', 'prod-proxy')
    vi.stubEnv('VITE_DEV_PROXY_ALLOW_WRITES', allowWrites)
    vi.stubEnv('VITE_DEV_PROXY_TARGET', 'https://prod.example.com')

    const { unmount } = render(<DevProxyBanner />)
    expect(screen.getByRole('alert')).toHaveTextContent('https://prod.example.com')

    fireEvent.click(screen.getByRole('button', { name: 'Close' }))
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()

    unmount()
    render(<DevProxyBanner />)
    expect(screen.getByRole('alert')).toBeInTheDocument()
  })
})
