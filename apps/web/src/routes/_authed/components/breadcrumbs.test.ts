import { describe, expect, it } from 'vitest'
import { buildBreadcrumbs, getServerDetailId } from './breadcrumbs'

const LABELS: Record<string, string> = {
  nav_dashboard: 'Dashboard',
  nav_servers: 'Servers',
  nav_settings: 'Settings',
  nav_users: 'Users'
}

const translate = (key: string) => LABELS[key] ?? key

describe('buildBreadcrumbs', () => {
  it('adds a named server detail after the linked servers parent', () => {
    expect(buildBreadcrumbs('/servers/server-1', translate, 'test-server')).toEqual([
      { label: 'Servers', to: '/servers' },
      { label: 'test-server' }
    ])
  })

  it('uses the server id until detail data is available', () => {
    expect(buildBreadcrumbs('/servers/server-1', translate)).toEqual([
      { label: 'Servers', to: '/servers' },
      { label: 'server-1' }
    ])
  })

  it('keeps existing static breadcrumbs unchanged', () => {
    expect(buildBreadcrumbs('/settings/users', translate)).toEqual([
      { label: 'Settings', to: '/settings' },
      { label: 'Users' }
    ])
  })

  it('leaves the dashboard route without a header title crumb', () => {
    expect(buildBreadcrumbs('/', translate)).toEqual([])
  })
})

describe('getServerDetailId', () => {
  it('matches only the server detail route', () => {
    expect(getServerDetailId('/servers/server-1')).toBe('server-1')
    expect(getServerDetailId('/servers/server-1/')).toBe('server-1')
    expect(getServerDetailId('/servers/server-1/docker')).toBe('')
  })
})
