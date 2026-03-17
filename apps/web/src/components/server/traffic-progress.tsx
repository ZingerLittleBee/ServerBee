import { useTranslation } from 'react-i18next'
import { useTraffic } from '@/hooks/use-traffic'
import { cn, formatBytes } from '@/lib/utils'

export function TrafficProgress({ serverId }: { serverId: string }) {
  const { t } = useTranslation('servers')
  const { data } = useTraffic(serverId)

  if (!data || data.traffic_limit == null) {
    return null
  }

  const limit = data.traffic_limit
  const used = (() => {
    switch (data.traffic_limit_type) {
      case 'up':
        return data.bytes_out
      case 'down':
        return data.bytes_in
      default:
        return data.bytes_total
    }
  })()

  const percent = limit > 0 ? Math.min((used / limit) * 100, 100) : 0

  const barColor = (() => {
    if (percent >= 90) {
      return 'bg-red-500'
    }
    if (percent >= 70) {
      return 'bg-yellow-500'
    }
    return 'bg-green-500'
  })()

  const predictionPercent =
    data.prediction && limit > 0 ? Math.min((data.prediction.estimated_total / limit) * 100, 100) : null

  return (
    <div className="flex w-full min-w-48 flex-col gap-1">
      <div className="flex items-center justify-between text-muted-foreground text-xs">
        <span>
          {formatBytes(used)} / {formatBytes(limit)}
          {data.traffic_limit_type && data.traffic_limit_type !== 'sum' && (
            <span className="ml-1">({data.traffic_limit_type})</span>
          )}
        </span>
        <span className={cn(percent >= 90 ? 'text-red-500' : percent >= 70 ? 'text-yellow-500' : '')}>
          {percent.toFixed(1)}%
        </span>
      </div>
      <div className="relative h-2 w-full overflow-hidden rounded-full bg-muted">
        <div className={cn('h-full rounded-full transition-all', barColor)} style={{ width: `${percent}%` }} />
        {predictionPercent != null && predictionPercent > percent && (
          <div
            className="absolute top-0 h-full border-muted-foreground border-r-2 border-dashed"
            style={{ left: `${predictionPercent}%` }}
          />
        )}
      </div>
      {data.prediction?.will_exceed && (
        <div className="text-red-500 text-xs">
          {t('traffic_exceed_warning', {
            defaultValue: 'Predicted to exceed limit (~{{percent}}%)',
            percent: data.prediction.estimated_percent.toFixed(0)
          })}
        </div>
      )}
    </div>
  )
}
