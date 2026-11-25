import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { DashboardCard } from '@/components/dashboard/dashboard-card'
import { WidgetHeader, WidgetMessage, WidgetValue } from '@/components/dashboard/widget-chrome'
import { useServerRecords } from '@/hooks/use-api'
import { useMetricSeries } from '@/hooks/use-metric-series'
import type { ServerMetrics } from '@/lib/server-catalog'
import { cn } from '@/lib/utils'
import { normalizeWidgetColor } from '@/lib/widget-color'
import type { MetricCardConfig } from '@/lib/widget-types'
import { METRIC_CARD_ACCENT, METRIC_CARD_SPECS } from './metric-card/metric-card-config'
import { MetricCardSparkline } from './metric-card/metric-card-sparkline'
import { MetricCardDelta } from './metric-card/metric-card-value'

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
    enabled: Boolean(config.server_id) && Boolean(server) && Boolean(spec)
  })

  const series = useMetricSeries({ records, server, metric: config.metric })

  if (!spec) {
    return (
      <DashboardCard className="flex h-full bg-card" data-testid="metric-card-missing-metric">
        <WidgetMessage title={t('metricCard.unknownMetric')} />
      </DashboardCard>
    )
  }

  if (!server) {
    return (
      <DashboardCard className="flex h-full bg-card" data-testid="metric-card-missing-server">
        <WidgetMessage title={t('metricCard.unknownServer')} />
      </DashboardCard>
    )
  }

  const label = config.label ?? t(spec.labelKey)
  const dimmed = !server.online
  const formattedValue = dimmed ? '—' : spec.formatValue(series.current)
  const formattedPeak = formatStat(series.peak, spec.formatValue)
  const formattedAvg = formatStat(series.avg, spec.formatValue)
  // A custom hex wins; otherwise every metric uses the shared accent.
  const accent = normalizeWidgetColor(config.color) ?? METRIC_CARD_ACCENT

  return (
    <DashboardCard
      className={cn('flex h-full flex-col overflow-hidden bg-card', dimmed && 'opacity-70')}
      data-metric={config.metric}
      data-testid="metric-card-widget"
    >
      <div className="flex flex-col gap-2.5 px-5 pt-5">
        <WidgetHeader
          aside={t('widgets.common.range.hours', { count: HISTORY_HOURS })}
          context={server.name}
          title={label}
        />
        <WidgetValue className="text-[2.75rem]" data-testid="metric-card-value" formatted={formattedValue} />
        <div className="flex min-w-0 items-center justify-between gap-3">
          <MetricCardDelta
            delta={server.online ? series.oneHourDelta : null}
            deltaTone={spec.deltaTone}
            deltaUnit={spec.deltaUnit}
            pastLabel={t('metricCard.past1h')}
          />
          <p className="shrink-0 truncate text-muted-foreground text-xs tabular-nums">
            {t('metricCard.peak')}{' '}
            <span className="text-foreground/80" data-testid="metric-card-stat-peak">
              {formattedPeak}
            </span>
            {' · '}
            {t('metricCard.avg')}{' '}
            <span className="text-foreground/80" data-testid="metric-card-stat-avg">
              {formattedAvg}
            </span>
          </p>
        </div>
      </div>
      {/* Full-bleed: the trend runs to the card edges so the reading above stays the focus. */}
      <div className="mt-3 min-h-0 flex-1">
        <MetricCardSparkline accent={accent} points={series.points} />
      </div>
    </DashboardCard>
  )
}
