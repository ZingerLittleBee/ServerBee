import type * as React from 'react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { AnomalyTable } from '@/components/network/anomaly-table'
import { TargetCard } from '@/components/network/target-card'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { CHART_COLORS } from '@/lib/chart-colors'
import {
  formatLatency,
  formatPacketLoss,
  getProviderLabel,
  latencyColorClass,
  type NetworkProbeAnomaly,
  type NetworkTargetSummary
} from '@/lib/network-types'
import { cn } from '@/lib/utils'

// Both admin (`NetworkProbeAnomaly`) and public (`PublicNetworkProbeAnomaly`)
// have the same fields used here, so we accept the auth'd shape and rely on
// structural typing from callers.
type AnomalyLike =
  | NetworkProbeAnomaly
  | {
      anomaly_type: string
      target_id: string
      target_name: string
      timestamp: string
      value: number
    }

export interface NetworkDetailContentSummary {
  online: boolean
  server_id: string
  server_name: string
  targets: NetworkTargetSummary[]
}

export interface NetworkDetailContentProps {
  anomalies: AnomalyLike[]
  /** Whether the anomaly dialog is open. Controlled. */
  anomalyOpen: boolean
  /** Window in hours used for the anomaly dialog header. */
  anomalyWindowHours: number
  /** Admin-only: rendered between the target tabs and the bottom stats row.
   *  Public callers leave this undefined. */
  chartSlot?: React.ReactNode
  /** Admin-only: rendered above the target tabs (e.g. time-range buttons). */
  controlsSlot?: React.ReactNode
  /** Admin-only: rendered as the right-hand side of the bottom stats grid. */
  extraStatsSlot?: React.ReactNode
  /** Optional override mapping target metadata for localized display names. */
  getTargetDisplayName?: (target: NetworkTargetSummary) => string
  onAnomalyOpenChange: (open: boolean) => void
  /** Admin-only: callback to toggle per-target chart visibility. */
  onToggleTarget?: (targetId: string) => void
  summary: NetworkDetailContentSummary
  variant: 'admin' | 'public'
  /** Admin-only: which target series are currently visible in the chart. */
  visibleTargetIds?: Set<string>
}

const PROVIDER_KEYS = ['ct', 'cu', 'cm', 'international'] as const

const PROVIDER_TO_KEY: Record<string, string> = {
  Telecom: 'ct',
  Unicom: 'cu',
  Mobile: 'cm',
  International: 'international'
}

function groupTargetsByProvider(targets: NetworkTargetSummary[]) {
  const groups: Record<string, NetworkTargetSummary[]> = {}
  for (const target of targets) {
    const key = PROVIDER_TO_KEY[target.provider] || target.provider || 'unknown'
    if (!groups[key]) {
      groups[key] = []
    }
    groups[key].push(target)
  }
  return groups
}

function ProviderColumn({
  getTargetDisplayName,
  provider,
  targets,
  t
}: {
  getTargetDisplayName: (target: NetworkTargetSummary) => string
  provider: string
  targets: NetworkTargetSummary[]
  t: (key: string, options?: { defaultValue?: string }) => string
}) {
  const providerI18nKey = `provider_${provider}`
  const label = t(providerI18nKey, { defaultValue: getProviderLabel(provider) })

  const avgLatency = useMemo(() => {
    const valid = targets.filter((target) => target.avg_latency != null)
    if (valid.length === 0) {
      return null
    }
    return valid.reduce((sum, target) => sum + (target.avg_latency ?? 0), 0) / valid.length
  }, [targets])

  const avgPacketLoss = useMemo(() => {
    if (targets.length === 0) {
      return 0
    }
    return targets.reduce((sum, target) => sum + target.packet_loss, 0) / targets.length
  }, [targets])

  return (
    <Card>
      <CardHeader>
        <CardTitle>{label}</CardTitle>
        <div className="flex gap-3 text-muted-foreground text-xs">
          <span>
            {t('avg_latency')}: <span className="font-mono">{formatLatency(avgLatency)}</span>
          </span>
          <span>
            {t('packet_loss')}: <span className="font-mono">{formatPacketLoss(avgPacketLoss)}</span>
          </span>
        </div>
      </CardHeader>
      <CardContent>
        {targets.length === 0 ? (
          <p className="text-center text-muted-foreground text-sm">{t('no_data')}</p>
        ) : (
          <div className="space-y-2">
            {targets.map((target) => (
              <div
                className="flex items-center justify-between rounded-md border px-3 py-2 text-sm"
                key={target.target_id}
              >
                <span className="font-medium">{getTargetDisplayName(target)}</span>
                <div className="flex items-center gap-3 text-xs">
                  <span
                    className={cn(
                      'font-mono',
                      latencyColorClass(target.avg_latency, { failed: target.packet_loss >= 1 })
                    )}
                  >
                    {formatLatency(target.avg_latency)}
                  </span>
                  <span className="text-muted-foreground">{formatPacketLoss(target.packet_loss)}</span>
                </div>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  )
}

export function NetworkDetailContent(props: NetworkDetailContentProps) {
  const {
    anomalies,
    anomalyOpen,
    anomalyWindowHours,
    chartSlot,
    controlsSlot,
    extraStatsSlot,
    getTargetDisplayName,
    onAnomalyOpenChange,
    onToggleTarget,
    summary,
    visibleTargetIds
  } = props
  const { t } = useTranslation('network')

  const targets = summary.targets
  const providerGroups = useMemo(() => groupTargetsByProvider(targets), [targets])
  const orderedProviderKeys = useMemo(() => {
    const known = PROVIDER_KEYS.filter((k) => providerGroups[k]?.length)
    const remaining = Object.keys(providerGroups).filter(
      (k) => !PROVIDER_KEYS.includes(k as (typeof PROVIDER_KEYS)[number])
    )
    return [...known, ...remaining]
  }, [providerGroups])

  const targetColorMap = useMemo(() => {
    const map: Record<string, string> = {}
    for (let i = 0; i < targets.length; i++) {
      map[targets[i].target_id] = CHART_COLORS[i % CHART_COLORS.length]
    }
    return map
  }, [targets])

  const resolveDisplayName = getTargetDisplayName ?? ((target: NetworkTargetSummary) => target.target_name)

  return (
    <>
      {controlsSlot}

      {targets.length > 0 && (
        <Tabs className="mb-4" defaultValue="all">
          <TabsList>
            <TabsTrigger value="all">{t('all_targets')}</TabsTrigger>
            <TabsTrigger value="provider">{t('by_provider')}</TabsTrigger>
          </TabsList>

          <TabsContent value="all">
            <div className="flex flex-wrap gap-2 pt-2">
              {targets.map((target) => (
                <TargetCard
                  color={targetColorMap[target.target_id] ?? CHART_COLORS[0]}
                  displayName={resolveDisplayName(target)}
                  key={target.target_id}
                  onToggle={onToggleTarget ? () => onToggleTarget(target.target_id) : undefined}
                  target={target}
                  visible={visibleTargetIds ? visibleTargetIds.has(target.target_id) : true}
                />
              ))}
            </div>
          </TabsContent>

          <TabsContent value="provider">
            <div className="grid gap-4 pt-2 md:grid-cols-2 lg:grid-cols-3">
              {orderedProviderKeys.map((provider) => (
                <ProviderColumn
                  getTargetDisplayName={resolveDisplayName}
                  key={provider}
                  provider={provider}
                  t={t}
                  targets={providerGroups[provider]}
                />
              ))}
            </div>
          </TabsContent>
        </Tabs>
      )}

      {chartSlot}

      {extraStatsSlot}

      {/* Anomaly Dialog — shared read-only view. AnomalyTable renders an
          actions column only when caller provides delete callbacks; here we
          intentionally pass none so both variants render read-only rows. */}
      <Dialog onOpenChange={onAnomalyOpenChange} open={anomalyOpen}>
        <DialogContent className="sm:max-w-3xl">
          <DialogHeader>
            <div className="flex items-center justify-between gap-4 pr-8">
              <DialogTitle>{t('anomaly_count_with_value', { count: anomalies.length })}</DialogTitle>
              <span className="text-muted-foreground text-xs">
                {t('anomaly_window', { hours: anomalyWindowHours })}
              </span>
            </div>
          </DialogHeader>
          <ScrollArea className="max-h-[70vh]">
            <AnomalyTable anomalies={anomalies as NetworkProbeAnomaly[]} windowHours={anomalyWindowHours} />
          </ScrollArea>
        </DialogContent>
      </Dialog>
    </>
  )
}
