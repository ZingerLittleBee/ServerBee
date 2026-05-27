import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useServerRecords } from '@/hooks/use-api'
import { useMetricSeries } from '@/hooks/use-metric-series'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { cn } from '@/lib/utils'
import type { MetricCardConfig } from '@/lib/widget-types'
import { METRIC_CARD_SPECS } from './metric-card/metric-card-config'
import { MetricCardHeader } from './metric-card/metric-card-header'
import { MetricCardSparkline } from './metric-card/metric-card-sparkline'
import { MetricCardStats } from './metric-card/metric-card-stats'
import { MetricCardValue } from './metric-card/metric-card-value'

interface MetricCardWidgetProps {
  config: MetricCardConfig
  servers: ServerMetrics[]
}

const HISTORY_HOURS = 24
const HISTORY_INTERVAL = '5m'

function formatStat(value: number | null, formatter: (n: number) => string): string {
  return value === null ? '—' : formatter(value)
}

export function MetricCardWidget({ config, servers }: MetricCardWidgetProps) {
  const { t } = useTranslation('dashboard')
  const spec = METRIC_CARD_SPECS[config.metric]
  const server = useMemo(() => servers.find((s) => s.id === config.server_id), [servers, config.server_id])

  const { data: records } = useServerRecords(config.server_id, HISTORY_HOURS, HISTORY_INTERVAL, {
    enabled: Boolean(config.server_id) && Boolean(server)
  })

  const series = useMetricSeries({ records, server, metric: config.metric })

  if (!server) {
    return (
      <div
        className="flex h-full items-center justify-center rounded-xl border bg-card text-muted-foreground text-sm"
        data-testid="metric-card-missing-server"
      >
        {t('metricCard.unknownServer')}
      </div>
    )
  }

  const label = config.label ?? t(spec.labelKey)
  const dimmed = !server.online
  const formattedValue = dimmed ? '—' : spec.formatValue(series.current)
  const formattedPeak = formatStat(series.peak, spec.formatValue)
  const formattedAvg = formatStat(series.avg, spec.formatValue)

  return (
    <div
      className={cn(
        'flex h-full min-w-0 flex-col gap-3 overflow-hidden rounded-xl border bg-card p-3 shadow-sm',
        dimmed && 'opacity-70'
      )}
      data-metric={config.metric}
      data-testid="metric-card-widget"
    >
      <MetricCardHeader accent={spec.accent} Icon={spec.icon} label={label} serverName={server.name} />
      <MetricCardValue
        delta={server.online ? series.oneHourDelta : null}
        deltaTone={spec.deltaTone}
        deltaUnit={spec.deltaUnit}
        formattedValue={formattedValue}
        pastLabel={t('metricCard.past1h')}
      />
      <div className="min-h-0 flex-1">
        <MetricCardSparkline accent={spec.accent} points={series.points} />
      </div>
      <MetricCardStats
        avg={formattedAvg}
        avgCaption={t('metricCard.avg')}
        peak={formattedPeak}
        peakCaption={t('metricCard.peak')}
      />
    </div>
  )
}
