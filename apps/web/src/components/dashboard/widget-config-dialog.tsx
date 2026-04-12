import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { renderMarkdown } from '@/lib/markdown'
import { parseConfig } from '@/lib/widget-helpers'
import type {
  AlertListConfig,
  DashboardWidget,
  DiskIoConfig,
  GaugeConfig,
  LineChartConfig,
  MarkdownConfig,
  MultiLineConfig,
  ServerCardsConfig,
  StatNumberConfig,
  TopNConfig,
  TrafficBarConfig,
  UptimeTimelineConfig,
  WidgetConfig
} from '@/lib/widget-types'

interface WidgetConfigDialogProps {
  onOpenChange: (open: boolean) => void
  onSubmit: (title: string, configJson: string) => void
  open: boolean
  servers: ServerMetrics[]
  widget?: DashboardWidget
  widgetType: string
}

// Metric options for stat-number
function useStatMetrics(t: (key: string) => string): { label: string; value: string }[] {
  return [
    { label: t('common.metrics.serverCount'), value: 'server_count' },
    { label: t('common.metrics.avgCpu'), value: 'avg_cpu' },
    { label: t('common.metrics.avgMemory'), value: 'avg_memory' },
    { label: t('common.metrics.totalBandwidth'), value: 'total_bandwidth' },
    { label: t('common.metrics.health'), value: 'health' }
  ]
}

// Metric options for gauge / line-chart / multi-line
function useGaugeMetrics(t: (key: string) => string): { label: string; value: string }[] {
  return [
    { label: t('common.metrics.cpu'), value: 'cpu' },
    { label: t('common.metrics.memory'), value: 'memory' },
    { label: t('common.metrics.disk'), value: 'disk' },
    { label: t('common.metrics.swap'), value: 'swap' }
  ]
}

function useLineMetrics(t: (key: string) => string): { label: string; value: string }[] {
  return [
    { label: t('common.metrics.cpu'), value: 'cpu' },
    { label: t('common.metrics.memory'), value: 'memory' },
    { label: t('common.metrics.disk'), value: 'disk' },
    { label: t('common.metrics.load1m'), value: 'load1' },
    { label: t('common.metrics.load5m'), value: 'load5' },
    { label: t('common.metrics.load15m'), value: 'load15' },
    { label: t('common.metrics.networkIn'), value: 'net_in' },
    { label: t('common.metrics.networkOut'), value: 'net_out' }
  ]
}

function useTopNMetrics(t: (key: string) => string): { label: string; value: string }[] {
  return [
    { label: t('common.metrics.cpu'), value: 'cpu' },
    { label: t('common.metrics.memory'), value: 'memory' },
    { label: t('common.metrics.disk'), value: 'disk' },
    { label: t('common.metrics.bandwidth'), value: 'bandwidth' }
  ]
}

function useRangeOptions(t: (key: string) => string): { label: string; value: string }[] {
  return [
    { label: t('common.timeRange.1hour'), value: '1' },
    { label: t('common.timeRange.6hours'), value: '6' },
    { label: t('common.timeRange.12hours'), value: '12' },
    { label: t('common.timeRange.24hours'), value: '24' },
    { label: t('common.timeRange.3days'), value: '72' },
    { label: t('common.timeRange.7days'), value: '168' }
  ]
}

function parseExistingConfig(widget?: DashboardWidget): WidgetConfig | null {
  if (!widget) {
    return null
  }
  return parseConfig<WidgetConfig>(widget.config_json)
}

function ServerSelect({
  label,
  servers,
  value,
  onChange,
  placeholder
}: {
  label: string
  onChange: (v: string) => void
  placeholder: string
  servers: ServerMetrics[]
  value: string
}) {
  return (
    <div className="space-y-1.5">
      <Label>{label}</Label>
      <Select
        items={servers.map((s) => ({ value: s.id, label: s.name }))}
        onValueChange={(v) => v !== null && onChange(v)}
        value={value}
      >
        <SelectTrigger className="w-full">
          <SelectValue placeholder={placeholder} />
        </SelectTrigger>
        <SelectContent>
          {servers.map((s) => (
            <SelectItem key={s.id} value={s.id}>
              {s.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}

function MetricSelect({
  label,
  metrics,
  value,
  onChange,
  placeholder
}: {
  label: string
  metrics: { label: string; value: string }[]
  onChange: (v: string) => void
  placeholder: string
  value: string
}) {
  return (
    <div className="space-y-1.5">
      <Label>{label}</Label>
      <Select items={metrics} onValueChange={(v) => v !== null && onChange(v)} value={value}>
        <SelectTrigger className="w-full">
          <SelectValue placeholder={placeholder} />
        </SelectTrigger>
        <SelectContent>
          {metrics.map((m) => (
            <SelectItem key={m.value} value={m.value}>
              {m.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}

function RangeSelect({
  value,
  onChange,
  t
}: {
  onChange: (v: string) => void
  t: (key: string) => string
  value: string
}) {
  const RANGE_OPTIONS = useRangeOptions(t)
  return (
    <div className="space-y-1.5">
      <Label>{t('widgets.common.labels.timeRange')}</Label>
      <Select items={RANGE_OPTIONS} onValueChange={(v) => v !== null && onChange(v)} value={value}>
        <SelectTrigger className="w-full">
          <SelectValue placeholder={t('widgets.common.placeholders.selectRange')} />
        </SelectTrigger>
        <SelectContent>
          {RANGE_OPTIONS.map((r) => (
            <SelectItem key={r.value} value={r.value}>
              {r.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}

function ServerMultiSelect({
  label,
  servers,
  selected,
  onChange,
  emptyMessage
}: {
  emptyMessage: string
  label: string
  onChange: (ids: string[]) => void
  selected: string[]
  servers: ServerMetrics[]
}) {
  const selectedSet = useMemo(() => new Set(selected), [selected])

  const toggle = (id: string) => {
    if (selectedSet.has(id)) {
      onChange(selected.filter((s) => s !== id))
    } else {
      onChange([...selected, id])
    }
  }

  return (
    <div className="space-y-1.5">
      <Label>{label}</Label>
      <div className="max-h-40 space-y-1 overflow-y-auto rounded-lg border p-2">
        {servers.map((s) => (
          <button
            className="flex w-full cursor-pointer items-center gap-2 rounded px-1 py-0.5 text-left hover:bg-muted/50"
            key={s.id}
            onClick={() => toggle(s.id)}
            type="button"
          >
            <Checkbox checked={selectedSet.has(s.id)} onCheckedChange={() => toggle(s.id)} />
            <span className="text-sm">{s.name}</span>
          </button>
        ))}
        {servers.length === 0 && <p className="py-2 text-center text-muted-foreground text-xs">{emptyMessage}</p>}
      </div>
    </div>
  )
}

// Individual config forms per widget type

function StatNumberForm({
  config,
  onChange,
  t
}: {
  config: Partial<StatNumberConfig>
  onChange: (c: Partial<StatNumberConfig>) => void
  t: (key: string) => string
}) {
  const STAT_METRICS = useStatMetrics(t)
  return (
    <MetricSelect
      label={t('widgets.common.labels.metric')}
      metrics={STAT_METRICS}
      onChange={(v) => onChange({ ...config, metric: v })}
      placeholder={t('widgets.common.placeholders.selectMetric')}
      value={config.metric ?? ''}
    />
  )
}

function GaugeForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<GaugeConfig>
  onChange: (c: Partial<GaugeConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  const GAUGE_METRICS = useGaugeMetrics(t)
  return (
    <>
      <ServerSelect
        label={t('widgets.common.labels.server')}
        onChange={(v) => onChange({ ...config, server_id: v })}
        placeholder={t('widgets.common.placeholders.selectServer')}
        servers={servers}
        value={config.server_id ?? ''}
      />
      <MetricSelect
        label={t('widgets.common.labels.metric')}
        metrics={GAUGE_METRICS}
        onChange={(v) => onChange({ ...config, metric: v })}
        placeholder={t('widgets.common.placeholders.selectMetric')}
        value={config.metric ?? ''}
      />
    </>
  )
}

function LineChartForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<LineChartConfig>
  onChange: (c: Partial<LineChartConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  const LINE_METRICS = useLineMetrics(t)
  return (
    <>
      <ServerSelect
        label={t('widgets.common.labels.server')}
        onChange={(v) => onChange({ ...config, server_id: v })}
        placeholder={t('widgets.common.placeholders.selectServer')}
        servers={servers}
        value={config.server_id ?? ''}
      />
      <MetricSelect
        label={t('widgets.common.labels.metric')}
        metrics={LINE_METRICS}
        onChange={(v) => onChange({ ...config, metric: v })}
        placeholder={t('widgets.common.placeholders.selectMetric')}
        value={config.metric ?? ''}
      />
      <RangeSelect
        onChange={(v) => onChange({ ...config, hours: Number(v) })}
        t={t}
        value={String(config.hours ?? '24')}
      />
    </>
  )
}

function MultiLineForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<MultiLineConfig>
  onChange: (c: Partial<MultiLineConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  const LINE_METRICS = useLineMetrics(t)
  return (
    <>
      <ServerMultiSelect
        emptyMessage={t('widgets.common.empty.noServers')}
        label={t('widgets.common.labels.servers')}
        onChange={(ids) => onChange({ ...config, server_ids: ids })}
        selected={config.server_ids ?? []}
        servers={servers}
      />
      <MetricSelect
        label={t('widgets.common.labels.metric')}
        metrics={LINE_METRICS}
        onChange={(v) => onChange({ ...config, metric: v })}
        placeholder={t('widgets.common.placeholders.selectMetric')}
        value={config.metric ?? ''}
      />
      <RangeSelect
        onChange={(v) => onChange({ ...config, hours: Number(v) })}
        t={t}
        value={String(config.hours ?? '24')}
      />
    </>
  )
}

function TopNForm({
  config,
  onChange,
  t
}: {
  config: Partial<TopNConfig>
  onChange: (c: Partial<TopNConfig>) => void
  t: (key: string) => string
}) {
  const TOP_N_METRICS = useTopNMetrics(t)
  return (
    <>
      <MetricSelect
        label={t('widgets.common.labels.metric')}
        metrics={TOP_N_METRICS}
        onChange={(v) => onChange({ ...config, metric: v })}
        placeholder={t('widgets.common.placeholders.selectMetric')}
        value={config.metric ?? ''}
      />
      <div className="space-y-1.5">
        <Label>{t('widgets.common.labels.count')}</Label>
        <Input
          max={20}
          min={1}
          onChange={(e) => onChange({ ...config, count: Number(e.target.value) || 5 })}
          type="number"
          value={config.count ?? 5}
        />
      </div>
    </>
  )
}

function ServerCardsForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<ServerCardsConfig>
  onChange: (c: Partial<ServerCardsConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  return (
    <ServerMultiSelect
      emptyMessage={t('widgets.common.empty.noServers')}
      label={t('widgets.common.labels.servers')}
      onChange={(ids) => onChange({ ...config, server_ids: ids })}
      selected={config.server_ids ?? []}
      servers={servers}
    />
  )
}

function TrafficBarForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<TrafficBarConfig>
  onChange: (c: Partial<TrafficBarConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  return (
    <>
      <div className="space-y-1.5">
        <Label>{t('widgets.common.labels.server')}</Label>
        <Select
          items={[
            { value: '__all__', label: t('widgets.common.placeholders.allServers') },
            ...servers.map((s) => ({ value: s.id, label: s.name }))
          ]}
          onValueChange={(v) => onChange({ ...config, server_id: v === '__all__' ? '' : (v ?? '') })}
          value={config.server_id || '__all__'}
        >
          <SelectTrigger className="w-full">
            <SelectValue placeholder={t('widgets.common.placeholders.allServers')} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{t('widgets.common.placeholders.allServers')}</SelectItem>
            {servers.map((s) => (
              <SelectItem key={s.id} value={s.id}>
                {s.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <RangeSelect
        onChange={(v) => onChange({ ...config, hours: Number(v) })}
        t={t}
        value={String(config.hours ?? '720')}
      />
    </>
  )
}

function DiskIoForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<DiskIoConfig>
  onChange: (c: Partial<DiskIoConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  return (
    <>
      <ServerSelect
        label={t('widgets.common.labels.server')}
        onChange={(v) => onChange({ ...config, server_id: v })}
        placeholder={t('widgets.common.placeholders.selectServer')}
        servers={servers}
        value={config.server_id ?? ''}
      />
      <RangeSelect
        onChange={(v) => onChange({ ...config, hours: Number(v) })}
        t={t}
        value={String(config.hours ?? '24')}
      />
    </>
  )
}

function AlertListForm({
  config,
  onChange,
  t
}: {
  config: Partial<AlertListConfig>
  onChange: (c: Partial<AlertListConfig>) => void
  t: (key: string) => string
}) {
  return (
    <div className="space-y-1.5">
      <Label>{t('widgets.common.labels.maxItems')}</Label>
      <Input
        max={50}
        min={1}
        onChange={(e) => onChange({ ...config, max_items: Number(e.target.value) || 10 })}
        type="number"
        value={config.max_items ?? 10}
      />
    </div>
  )
}

function MarkdownForm({
  config,
  onChange,
  t
}: {
  config: Partial<MarkdownConfig>
  onChange: (c: Partial<MarkdownConfig>) => void
  t: (key: string) => string
}) {
  const html = useMemo(() => renderMarkdown(config.content ?? ''), [config.content])

  return (
    <div className="space-y-1.5">
      <Label>{t('widgets.common.labels.markdownContent')}</Label>
      <textarea
        className="h-32 w-full rounded-lg border border-input bg-transparent px-2.5 py-2 text-sm outline-none transition-colors focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30"
        onChange={(e) => onChange({ ...config, content: e.target.value })}
        placeholder={t('widgets.common.placeholders.writeMarkdown')}
        value={config.content ?? ''}
      />
      {(config.content ?? '').length > 0 && (
        <div className="rounded-lg border p-3">
          <p className="mb-1 font-medium text-muted-foreground text-xs">{t('dialogs.widgetConfig.labels.preview')}</p>
          <div
            className="prose prose-sm dark:prose-invert max-h-32 max-w-none overflow-auto text-sm"
            // biome-ignore lint/security/noDangerouslySetInnerHtml: renderMarkdown escapes all raw HTML and validates URLs
            dangerouslySetInnerHTML={{ __html: html }}
          />
        </div>
      )}
    </div>
  )
}

function useUptimeDaysOptions(t: (key: string) => string): { label: string; value: string }[] {
  return [
    { label: t('common.timeRange.30days'), value: '30' },
    { label: t('common.timeRange.60days'), value: '60' },
    { label: t('common.timeRange.90days'), value: '90' }
  ]
}

function UptimeTimelineForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<UptimeTimelineConfig>
  onChange: (c: Partial<UptimeTimelineConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  const UPTIME_DAYS_OPTIONS = useUptimeDaysOptions(t)
  return (
    <>
      <ServerMultiSelect
        emptyMessage={t('widgets.common.empty.noServers')}
        label={t('widgets.common.labels.servers')}
        onChange={(ids) => onChange({ ...config, server_ids: ids })}
        selected={config.server_ids ?? []}
        servers={servers}
      />
      <div className="space-y-1.5">
        <Label>{t('widgets.common.labels.days')}</Label>
        <Select
          items={UPTIME_DAYS_OPTIONS}
          onValueChange={(v) => v !== null && onChange({ ...config, days: Number(v) })}
          value={String(config.days ?? '90')}
        >
          <SelectTrigger className="w-full">
            <SelectValue placeholder={t('widgets.common.placeholders.selectRange')} />
          </SelectTrigger>
          <SelectContent>
            {UPTIME_DAYS_OPTIONS.map((r) => (
              <SelectItem key={r.value} value={r.value}>
                {r.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    </>
  )
}

export function WidgetConfigDialog({
  open,
  onOpenChange,
  onSubmit,
  widgetType,
  widget,
  servers
}: WidgetConfigDialogProps) {
  const { t } = useTranslation('dashboard')

  const [title, setTitle] = useState(widget?.title ?? '')
  const [config, setConfig] = useState<Record<string, unknown>>(
    () => (parseExistingConfig(widget) as Record<string, unknown>) ?? {}
  )

  useEffect(() => {
    setTitle(widget?.title ?? '')
    setConfig((parseExistingConfig(widget) as Record<string, unknown>) ?? {})
  }, [widget])

  const needsNoConfig = widgetType === 'service-status' || widgetType === 'server-map'

  const handleSubmit = () => {
    onSubmit(title, JSON.stringify(config))
    onOpenChange(false)
  }

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {widget ? t('dialogs.widgetConfig.editTitle') : t('dialogs.widgetConfig.configureTitle')}
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-4">
          <div className="space-y-1.5">
            <Label>{t('dialogs.widgetConfig.labels.titleOptional')}</Label>
            <Input
              onChange={(e) => setTitle(e.target.value)}
              placeholder={t('dialogs.widgetConfig.placeholders.widgetTitle')}
              value={title}
            />
          </div>

          {widgetType === 'stat-number' && (
            <StatNumberForm config={config as Partial<StatNumberConfig>} onChange={setConfig} t={t} />
          )}
          {widgetType === 'gauge' && (
            <GaugeForm config={config as Partial<GaugeConfig>} onChange={setConfig} servers={servers} t={t} />
          )}
          {widgetType === 'line-chart' && (
            <LineChartForm config={config as Partial<LineChartConfig>} onChange={setConfig} servers={servers} t={t} />
          )}
          {widgetType === 'multi-line' && (
            <MultiLineForm config={config as Partial<MultiLineConfig>} onChange={setConfig} servers={servers} t={t} />
          )}
          {widgetType === 'top-n' && <TopNForm config={config as Partial<TopNConfig>} onChange={setConfig} t={t} />}
          {widgetType === 'server-cards' && (
            <ServerCardsForm
              config={config as Partial<ServerCardsConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {widgetType === 'traffic-bar' && (
            <TrafficBarForm config={config as Partial<TrafficBarConfig>} onChange={setConfig} servers={servers} t={t} />
          )}
          {widgetType === 'disk-io' && (
            <DiskIoForm config={config as Partial<DiskIoConfig>} onChange={setConfig} servers={servers} t={t} />
          )}
          {widgetType === 'alert-list' && (
            <AlertListForm config={config as Partial<AlertListConfig>} onChange={setConfig} t={t} />
          )}
          {widgetType === 'markdown' && (
            <MarkdownForm config={config as Partial<MarkdownConfig>} onChange={setConfig} t={t} />
          )}
          {widgetType === 'uptime-timeline' && (
            <UptimeTimelineForm
              config={config as Partial<UptimeTimelineConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {needsNoConfig && (
            <p className="text-muted-foreground text-sm">{t('dialogs.widgetConfig.messages.noConfigNeeded')}</p>
          )}
        </div>
        <DialogFooter>
          <Button onClick={handleSubmit}>{widget ? t('save') : t('add_widget')}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
