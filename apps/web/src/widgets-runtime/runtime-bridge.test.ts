// biome-ignore lint/performance/noNamespaceImport: shim-drift test enumerates every named export.
import * as Sdk from '@serverbee/widget-sdk'
import { QueryClient } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { mountRuntimeBridge } from './runtime-bridge'

function makeServer(id: string): ServerMetrics {
  return {
    id,
    name: id,
    cpu: 0,
    cpu_cores: null,
    cpu_name: null,
    mem_total: 0,
    mem_used: 0,
    swap_total: 0,
    swap_used: 0,
    disk_total: 0,
    disk_used: 0,
    disk_read_bytes_per_sec: 0,
    disk_write_bytes_per_sec: 0,
    net_in_speed: 0,
    net_in_transfer: 0,
    net_out_speed: 0,
    net_out_transfer: 0,
    load1: 0,
    load5: 0,
    load15: 0,
    process_count: 0,
    tcp_conn: 0,
    udp_conn: 0,
    uptime: 0,
    last_active: 0,
    online: true,
    country_code: null,
    group_id: null,
    os: null,
    region: null,
    capabilities: 0
  }
}

describe('runtime-bridge: React Query store wiring', () => {
  let qc: QueryClient

  beforeEach(() => {
    Sdk.resetRuntime()
    qc = new QueryClient()
    mountRuntimeBridge({ queryClient: qc })
  })

  afterEach(() => {
    Sdk.resetRuntime()
  })

  it('serversStore is empty until the cache is seeded', () => {
    expect(Sdk.getRuntime().serversStore()).toEqual([])
  })

  it('serversStore reflects current cache after setQueryData', () => {
    qc.setQueryData<ServerMetrics[]>(['servers'], [makeServer('s1'), makeServer('s2')])
    const list = Sdk.getRuntime().serversStore()
    expect(list.map((s) => s.id)).toEqual(['s1', 's2'])
  })

  it('serversStore returns the same array reference when cache is unchanged', () => {
    qc.setQueryData<ServerMetrics[]>(['servers'], [makeServer('s1')])
    const a = Sdk.getRuntime().serversStore()
    const b = Sdk.getRuntime().serversStore()
    expect(a).toBe(b)
  })

  it('subscribeServers fires on ["servers"] cache updates', () => {
    const cb = vi.fn()
    const unsub = Sdk.getRuntime().subscribeServers(cb)
    qc.setQueryData<ServerMetrics[]>(['servers'], [makeServer('s1')])
    expect(cb).toHaveBeenCalled()
    unsub()
  })

  it('subscribeServers ignores updates to other query keys', () => {
    const cb = vi.fn()
    const unsub = Sdk.getRuntime().subscribeServers(cb)
    qc.setQueryData<ServerMetrics[]>(['something-else'], [makeServer('s1')])
    expect(cb).not.toHaveBeenCalled()
    unsub()
  })

  it('serverByIdStore looks up by id from cache', () => {
    qc.setQueryData<ServerMetrics[]>(['servers'], [makeServer('s1'), makeServer('s2')])
    const lookup = Sdk.getRuntime().serverByIdStore('s2')
    expect((lookup as ServerMetrics).id).toBe('s2')
    expect(Sdk.getRuntime().serverByIdStore('missing')).toBeUndefined()
  })

  it('notify routes through sonner without throwing', () => {
    // sonner is mocked-friendly: just confirm the call path runs.
    expect(() => Sdk.getRuntime().notify?.({ type: 'success', message: 'ok' })).not.toThrow()
    expect(() => Sdk.getRuntime().notify?.({ type: 'error', message: 'bad' })).not.toThrow()
  })

  it('themeStore reports light/dark from <html> class', () => {
    document.documentElement.classList.remove('dark')
    expect(Sdk.getRuntime().themeStore().mode).toBe('light')
    document.documentElement.classList.add('dark')
    expect(Sdk.getRuntime().themeStore().mode).toBe('dark')
    document.documentElement.classList.remove('dark')
  })
})

describe('runtime-bridge: shim export drift', () => {
  it('every named export listed in /runtime/widget-sdk.js resolves to a live SDK export', async () => {
    const fs = await import('node:fs/promises')
    const path = await import('node:path')
    const shimPath = path.resolve(import.meta.dirname, '../../public/runtime/widget-sdk.js')
    const shimSrc = await fs.readFile(shimPath, 'utf8')
    // Grep `export const <name> = ns.<rhsName>` pairs.
    const re = /export\s+const\s+(\w+)\s*=\s*ns\.(\w+)/g
    const pairs: Array<{ exportName: string; rhsName: string }> = []
    let match: RegExpExecArray | null = re.exec(shimSrc)
    while (match !== null) {
      pairs.push({ exportName: match[1], rhsName: match[2] })
      match = re.exec(shimSrc)
    }
    expect(pairs.length).toBeGreaterThan(0)
    const sdkAsRecord = Sdk as unknown as Record<string, unknown>
    for (const { exportName, rhsName } of pairs) {
      expect(rhsName, `shim exports ${exportName} from ns.${rhsName}`).toBe(exportName)
      expect(sdkAsRecord[rhsName], `SDK is missing export '${rhsName}' referenced by shim`).not.toBeUndefined()
    }
  })
})
