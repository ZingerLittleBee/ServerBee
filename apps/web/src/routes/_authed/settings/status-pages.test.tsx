import { describe, expect, it, vi } from 'vitest'

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (config: Record<string, unknown>) => config
}))

const { buildStatusPageUpdatePayload, parseServerIds } = await import('./status-page-config-utils')

describe('parseServerIds', () => {
  it('returns an empty array for null / undefined / empty input', () => {
    expect(parseServerIds(null)).toEqual([])
    expect(parseServerIds(undefined)).toEqual([])
    expect(parseServerIds('')).toEqual([])
  })

  it('returns an empty array for malformed JSON', () => {
    expect(parseServerIds('not json')).toEqual([])
    expect(parseServerIds('{}')).toEqual([])
  })

  it('decodes a JSON-encoded string array', () => {
    expect(parseServerIds('["srv-1","srv-2"]')).toEqual(['srv-1', 'srv-2'])
  })

  it('drops non-string entries', () => {
    expect(parseServerIds('["srv-1",42,null,"srv-2"]')).toEqual(['srv-1', 'srv-2'])
  })
})

describe('buildStatusPageUpdatePayload', () => {
  it('serialises every form field into the update DTO', () => {
    expect(
      buildStatusPageUpdatePayload({
        defaultLayout: 'grid',
        description: 'Hi',
        enabled: true,
        redThreshold: 95,
        selectedServers: ['srv-1', 'srv-2'],
        showIncidents: true,
        showIpQuality: false,
        showMaintenance: true,
        showNetwork: true,
        showServerDetail: false,
        title: 'My Status',
        yellowThreshold: 99
      })
    ).toEqual({
      default_layout: 'grid',
      description: 'Hi',
      enabled: true,
      server_ids: ['srv-1', 'srv-2'],
      show_incidents: true,
      show_ip_quality: false,
      show_maintenance: true,
      show_network: true,
      show_server_detail: false,
      title: 'My Status',
      uptime_red_threshold: 95,
      uptime_yellow_threshold: 99
    })
  })

  it('coerces a blank description to null', () => {
    const payload = buildStatusPageUpdatePayload({
      defaultLayout: 'list',
      description: '   ',
      enabled: false,
      redThreshold: 90,
      selectedServers: [],
      showIncidents: false,
      showIpQuality: false,
      showMaintenance: false,
      showNetwork: false,
      showServerDetail: true,
      title: 'X',
      yellowThreshold: 98
    })
    expect(payload.description).toBeNull()
  })

  it('preserves the disabled flag so the admin can save an off site', () => {
    const payload = buildStatusPageUpdatePayload({
      defaultLayout: 'list',
      description: '',
      enabled: false,
      redThreshold: 95,
      selectedServers: [],
      showIncidents: true,
      showIpQuality: true,
      showMaintenance: true,
      showNetwork: true,
      showServerDetail: true,
      title: 'Status',
      yellowThreshold: 99
    })
    expect(payload.enabled).toBe(false)
  })

  it('trims the title before sending it', () => {
    const payload = buildStatusPageUpdatePayload({
      defaultLayout: 'list',
      description: '',
      enabled: true,
      redThreshold: 95,
      selectedServers: [],
      showIncidents: true,
      showIpQuality: true,
      showMaintenance: true,
      showNetwork: true,
      showServerDetail: true,
      title: '  Trimmed  ',
      yellowThreshold: 99
    })
    expect(payload.title).toBe('Trimmed')
  })
})
