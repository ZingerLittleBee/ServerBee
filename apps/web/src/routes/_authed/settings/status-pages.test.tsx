import { describe, expect, it } from 'vitest'
import { buildStatusPagePayload } from './status-pages'

describe('buildStatusPagePayload', () => {
  it('includes theme_ref in status page submissions', () => {
    expect(
      buildStatusPagePayload({
        description: '',
        enabled: true,
        redThreshold: 95,
        selectedServers: ['srv-1'],
        slug: 'public',
        themeRef: 'custom:7',
        title: 'Public',
        yellowThreshold: 99
      })
    ).toEqual({
      description: null,
      enabled: true,
      server_ids: ['srv-1'],
      slug: 'public',
      theme_ref: 'custom:7',
      title: 'Public',
      uptime_red_threshold: 95,
      uptime_yellow_threshold: 99
    })
  })

  it('uses null theme_ref when a page follows the admin default', () => {
    expect(
      buildStatusPagePayload({
        description: 'Status',
        enabled: false,
        redThreshold: 90,
        selectedServers: [],
        slug: 'status',
        themeRef: null,
        title: 'Status',
        yellowThreshold: 98
      })
    ).toMatchObject({ theme_ref: null })
  })
})
