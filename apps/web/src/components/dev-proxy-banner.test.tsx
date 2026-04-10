import { describe, expect, it } from 'vitest'
import { getDevProxyBannerState } from './dev-proxy-banner'

describe('DevProxyBanner', () => {
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
})
