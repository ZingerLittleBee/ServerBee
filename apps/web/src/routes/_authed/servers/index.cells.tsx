import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { cn, formatBytes, formatSpeed } from '@/lib/utils'

function getBarColor(p: number): string {
  if (p > 90) {
    return 'bg-red-500'
  }
  if (p > 70) {
    return 'bg-amber-500'
  }
  return 'bg-emerald-500'
}

export function MiniBar({ pct, sub }: { pct: number; sub?: ReactNode }) {
  const p = Math.min(100, Math.max(0, pct))
  const color = getBarColor(p)
  return (
    <div className="min-w-[80px]">
      <div className="flex items-center gap-2">
        <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
          <div className={cn('h-full rounded-full', color)} style={{ width: `${p}%` }} />
        </div>
        <span className="w-10 text-right font-mono text-xs tabular-nums">{p.toFixed(0)}%</span>
      </div>
      {sub !== undefined && (
        <div className="mt-0.5 font-mono text-[10px] text-muted-foreground tabular-nums">{sub}</div>
      )}
    </div>
  )
}

export function CpuCell({ server }: { server: ServerMetrics }) {
  const { t } = useTranslation(['servers'])
  return (
    <MiniBar
      pct={server.cpu}
      sub={
        <span>
          {t('card_load')} {server.load1.toFixed(2)}
        </span>
      }
    />
  )
}

export function MemoryCell({ server }: { server: ServerMetrics }) {
  const pct = server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  return (
    <MiniBar
      pct={pct}
      sub={
        <span>
          {formatBytes(server.mem_used)} / {formatBytes(server.mem_total)}
        </span>
      }
    />
  )
}

export function DiskCell({ server }: { server: ServerMetrics }) {
  const pct = server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
  return (
    <MiniBar
      pct={pct}
      sub={
        <div className="flex flex-col gap-0.5">
          <span>
            {formatBytes(server.disk_used)} / {formatBytes(server.disk_total)}
          </span>
          {server.online && (
            <span className="inline-flex gap-2">
              <span>↺ {formatSpeed(server.disk_read_bytes_per_sec)}</span>
              <span>↻ {formatSpeed(server.disk_write_bytes_per_sec)}</span>
            </span>
          )}
        </div>
      }
    />
  )
}

export function NetworkCell({ server }: { server: ServerMetrics }) {
  const inSpeed = server.online ? server.net_in_speed : 0
  const outSpeed = server.online ? server.net_out_speed : 0

  return (
    <div className="flex flex-col gap-0.5 font-mono text-muted-foreground text-xs tabular-nums">
      <span className="inline-flex gap-2">
        <span className="inline-block min-w-[64px]">↓{formatSpeed(inSpeed)}</span>
        <span className="inline-block min-w-[64px]">↑{formatSpeed(outSpeed)}</span>
      </span>
      <span className="text-[10px]">
        <span>Σ ↓{formatBytes(server.net_in_transfer)}</span>
        <span className="ml-2">↑{formatBytes(server.net_out_transfer)}</span>
      </span>
    </div>
  )
}
