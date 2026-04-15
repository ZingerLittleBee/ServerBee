import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { cn } from '@/lib/utils'

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
