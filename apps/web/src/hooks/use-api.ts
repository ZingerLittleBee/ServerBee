import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'

interface ServerDetail {
  agent_version: string | null
  billing_cycle: string | null
  country_code: string | null
  cpu_arch: string | null
  cpu_cores: number | null
  cpu_name: string | null
  created_at: string
  currency: string | null
  disk_total: number | null
  expired_at: string | null
  group_id: string | null
  hidden: boolean
  id: string
  ipv4: string | null
  ipv6: string | null
  kernel_version: string | null
  mem_total: number | null
  name: string
  os: string | null
  price: number | null
  public_remark: string | null
  region: string | null
  remark: string | null
  swap_total: number | null
  traffic_limit: number | null
  traffic_limit_type: string | null
  updated_at: string
  virtualization: string | null
  weight: number
}

interface ServerRecord {
  cpu: number
  disk_used: number
  gpu_usage: number | null
  id: number
  load1: number
  load5: number
  load15: number
  mem_used: number
  net_in_speed: number
  net_in_transfer: number
  net_out_speed: number
  net_out_transfer: number
  process_count: number
  server_id: string
  swap_used: number
  tcp_conn: number
  temperature: number | null
  time: string
  udp_conn: number
}

export function useServer(id: string) {
  return useQuery<ServerDetail>({
    queryKey: ['servers', id],
    queryFn: () => api.get<ServerDetail>(`/api/servers/${id}`),
    enabled: id.length > 0
  })
}

export function useServerRecords(id: string, from: string, to: string, interval: string) {
  return useQuery<ServerRecord[]>({
    queryKey: ['servers', id, 'records', from, to, interval],
    queryFn: () =>
      api.get<ServerRecord[]>(
        `/api/servers/${id}/records?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&interval=${encodeURIComponent(interval)}`
      ),
    enabled: id.length > 0
  })
}

export type { ServerDetail, ServerRecord }
