import { describe, expect, it } from 'vitest'
import type { ServerMetrics } from './use-servers-ws'
import { mergeServerUpdate, setServerCapabilities, setServerOnlineStatus } from './use-servers-ws'

function makeServer(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 's1',
    name: 'Test',
    online: true,
    last_active: 0,
    cpu: 50,
    mem_used: 8_000_000_000,
    mem_total: 16_000_000_000,
    swap_used: 0,
    swap_total: 4_000_000_000,
    disk_used: 100_000_000_000,
    disk_total: 500_000_000_000,
    net_in_speed: 1000,
    net_out_speed: 500,
    net_in_transfer: 10_000,
    net_out_transfer: 5000,
    load1: 1.5,
    load5: 1.2,
    load15: 1.0,
    tcp_conn: 100,
    udp_conn: 10,
    process_count: 200,
    uptime: 3600,
    cpu_name: 'Intel i7',
    os: 'Linux',
    region: 'US-East',
    country_code: 'US',
    group_id: 'g1',
    ...overrides
  }
}

describe('mergeServerUpdate', () => {
  it('updates dynamic fields', () => {
    const prev = [makeServer({ cpu: 50 })]
    const incoming = [makeServer({ cpu: 75, mem_used: 10_000_000_000 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].cpu).toBe(75)
    expect(result[0].mem_used).toBe(10_000_000_000)
  })

  it('preserves static fields when incoming is null', () => {
    const prev = [makeServer({ mem_total: 16_000_000_000, os: 'Linux', cpu_name: 'Intel i7' })]
    const incoming = [makeServer({ id: 's1', mem_total: 0, os: null, cpu_name: null })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].mem_total).toBe(16_000_000_000)
    expect(result[0].os).toBe('Linux')
    expect(result[0].cpu_name).toBe('Intel i7')
  })

  it('preserves static fields when incoming is 0', () => {
    const prev = [makeServer({ disk_total: 500_000_000_000, swap_total: 4_000_000_000 })]
    const incoming = [makeServer({ id: 's1', disk_total: 0, swap_total: 0 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].disk_total).toBe(500_000_000_000)
    expect(result[0].swap_total).toBe(4_000_000_000)
  })

  it('ignores updates for unknown server id', () => {
    const prev = [makeServer({ id: 's1' })]
    const incoming = [makeServer({ id: 'unknown', cpu: 99 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result).toEqual(prev)
  })

  it('returns copy of prev when incoming is empty', () => {
    const prev = [makeServer()]
    const result = mergeServerUpdate(prev, [])
    expect(result).toEqual(prev)
  })
})

describe('setServerOnlineStatus', () => {
  it('sets target server offline', () => {
    const prev = [makeServer({ id: 's1', online: true }), makeServer({ id: 's2', online: true })]
    const result = setServerOnlineStatus(prev, 's1', false)
    expect(result[0].online).toBe(false)
    expect(result[1].online).toBe(true)
  })

  it('sets target server online', () => {
    const prev = [makeServer({ id: 's1', online: false })]
    const result = setServerOnlineStatus(prev, 's1', true)
    expect(result[0].online).toBe(true)
  })

  it('leaves all unchanged for unknown server', () => {
    const prev = [makeServer({ id: 's1', online: true })]
    const result = setServerOnlineStatus(prev, 'unknown', false)
    expect(result[0].online).toBe(true)
  })
})

describe('setServerCapabilities', () => {
  it('updates configured, local, and effective capabilities together', () => {
    const prev = [makeServer({ id: 's1' })]
    const result = setServerCapabilities(prev, 's1', 64, 0, 0)

    expect(result[0].capabilities).toBe(64)
    expect(result[0].agent_local_capabilities).toBe(0)
    expect(result[0].effective_capabilities).toBe(0)
  })
})

describe('setServerAgentVersion', () => {
  it('updates agent_version field', () => {
    const prev = [makeServer({ id: 's1', agent_version: undefined })]
    const result = prev.map((s) => (s.id === 's1' ? { ...s, agent_version: '1.2.3' } : s))

    expect(result[0].agent_version).toBe('1.2.3')
  })
})
