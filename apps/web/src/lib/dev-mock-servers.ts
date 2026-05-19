import type { ServerMetrics } from '@/hooks/use-servers-ws'

const STORAGE_KEY = 'serverbee-mock-servers'
const GIB = 1024 ** 3

function makeMockServer(index: number): ServerMetrics {
  const online = index % 5 !== 0
  const memTotal = 8 * GIB
  const diskTotal = 200 * GIB
  return {
    id: `mock-${index}`,
    name: `mock-server-${String(index).padStart(2, '0')}`,
    online,
    last_active: Math.floor(Date.now() / 1000),
    country_code: ['US', 'JP', 'DE', 'SG', 'HK'][index % 5],
    region: null,
    os: 'Ubuntu 24.04',
    cpu_name: 'Mock CPU',
    cpu: online ? (index * 7) % 100 : 0,
    cpu_cores: 4,
    load1: online ? (index % 4) + 0.2 : 0,
    load5: online ? (index % 3) + 0.1 : 0,
    load15: online ? (index % 2) + 0.05 : 0,
    mem_total: memTotal,
    mem_used: online ? memTotal * (((index * 13) % 90) / 100) : 0,
    swap_total: 2 * GIB,
    swap_used: 0,
    disk_total: diskTotal,
    disk_used: diskTotal * (((index * 17) % 80) / 100),
    disk_read_bytes_per_sec: 0,
    disk_write_bytes_per_sec: 0,
    net_in_speed: online ? index * 1024 : 0,
    net_out_speed: online ? index * 512 : 0,
    net_in_transfer: index * GIB,
    net_out_transfer: index * GIB,
    process_count: 120 + index,
    tcp_conn: 30 + index,
    udp_conn: 5 + (index % 7),
    uptime: 3600 * (index + 1),
    group_id: null,
    tags: index % 3 === 0 ? ['mock'] : []
  }
}

/**
 * DEV-only: appends generated mock servers when localStorage
 * `serverbee-mock-servers` holds a positive count. No-op in production
 * builds and when the flag is unset, so it never touches real data.
 */
export function withMockServers(servers: ServerMetrics[]): ServerMetrics[] {
  if (!import.meta.env.DEV || typeof window === 'undefined') {
    return servers
  }
  const raw = window.localStorage.getItem(STORAGE_KEY)
  const count = raw ? Number.parseInt(raw, 10) : 0
  if (!Number.isFinite(count) || count <= 0) {
    return servers
  }
  return [...servers, ...Array.from({ length: count }, (_, index) => makeMockServer(index + 1))]
}
