import { ConfigForm } from '@serverbee/widget-sdk'
import { useQuery } from '@tanstack/react-query'
import { LayoutGrid, List } from 'lucide-react'
import { useId, useMemo, useReducer } from 'react'
import { useTranslation } from 'react-i18next'
import { MarkdownContent } from '@/components/dashboard/markdown-content'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { api } from '@/lib/api-client'
import type { ServerMetrics } from '@/lib/server-catalog'
import { parseConfig } from '@/lib/widget-helpers'
import type {
  AlertListConfig,
  DashboardWidget,
  DiskIoConfig,
  GaugeConfig,
  LineChartConfig,
  MarkdownConfig,
  MetricCardConfig,
  MetricCardMetric,
  MultiLineConfig,
  NetworkLatencyConfig,
  NetworkOverviewConfig,
  NetworkQualityConfig,
  ServerCardsConfig,
  ServerCardsLayout,
  ServerMapConfig,
  ServiceStatusConfig,
  StatNumberConfig,
  TopNConfig,
  TrafficBarConfig,
  UptimeTimelineConfig,
  WidgetConfig
} from '@/lib/widget-types'
import { type RegistryEntry, registryActions } from '@/widgets-runtime/registry'

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

function useMetricCardMetrics(t: (key: string) => string): { label: string; value: MetricCardMetric }[] {
  return [
    { label: t('common.metrics.cpu'), value: 'cpu' },
    { label: t('common.metrics.memory'), value: 'memory' },
    { label: t('common.metrics.network'), value: 'network' },
    { label: t('common.metrics.diskIo'), value: 'disk_io' }
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

function useNetworkRangeOptions(t: (key: string) => string): { label: string; value: string }[] {
  return [
    { label: t('common.timeRange.realtime'), value: '0' },
    { label: t('common.timeRange.1hour'), value: '1' },
    { label: t('common.timeRange.6hours'), value: '6' },
    { label: t('common.timeRange.24hours'), value: '24' },
    { label: t('common.timeRange.7days'), value: '168' }
  ]
}

function parseExistingConfig(widget?: DashboardWidget): WidgetConfig | null {
  if (!widget) {
    return null
  }
  return parseConfig<WidgetConfig>(widget.config_json)
}

interface WidgetFormDraft {
  config: Record<string, unknown>
  title: string
}

type WidgetFormAction = { type: 'configChanged'; config: object } | { type: 'titleChanged'; value: string }

function toConfigRecord(config: object | null | undefined): Record<string, unknown> {
  return { ...config }
}

function createWidgetFormDraft(widget?: DashboardWidget): WidgetFormDraft {
  return {
    config: toConfigRecord(parseExistingConfig(widget)),
    title: widget?.title ?? ''
  }
}

function widgetFormReducer(state: WidgetFormDraft, action: WidgetFormAction): WidgetFormDraft {
  switch (action.type) {
    case 'configChanged':
      return { ...state, config: toConfigRecord(action.config) }
    case 'titleChanged':
      return { ...state, title: action.value }
    default:
      return state
  }
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
          <SelectValue placeholder={placeholder}>
            {(v: string | null) => servers.find((s) => s.id === v)?.name ?? placeholder}
          </SelectValue>
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
          <SelectValue placeholder={placeholder}>
            {(v: string | null) => metrics.find((m) => m.value === v)?.label ?? placeholder}
          </SelectValue>
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

function CheckboxMultiSelect({
  label,
  items,
  selected,
  onChange,
  emptyMessage,
  hint
}: {
  emptyMessage: string
  hint?: string
  items: { id: string; name: string }[]
  label: string
  onChange: (ids: string[]) => void
  selected: string[]
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
      <ScrollArea className="max-h-40 rounded-lg border" contentClassName="space-y-1 p-2">
        {items.map((item) => (
          <button
            className="flex w-full cursor-pointer items-center gap-2 rounded px-1 py-0.5 text-left hover:bg-muted/50"
            key={item.id}
            onClick={() => toggle(item.id)}
            type="button"
          >
            <Checkbox checked={selectedSet.has(item.id)} onCheckedChange={() => toggle(item.id)} />
            <span className="text-sm">{item.name}</span>
          </button>
        ))}
        {items.length === 0 && <p className="py-2 text-center text-muted-foreground text-xs">{emptyMessage}</p>}
      </ScrollArea>
      {hint && items.length > 0 && <p className="text-muted-foreground text-xs">{hint}</p>}
    </div>
  )
}

// Numeric config fields validate against these bounds; the dialog disables
// Save while any bound is violated. Values left empty are valid — the key is
// omitted from config_json and the widget's built-in default applies.
interface NumericBound {
  field: string
  integer: boolean
  max: number
  min: number
}

const GAUGE_MAX_BOUND: NumericBound = { field: 'max', min: 1, max: 100, integer: false }
const TOP_N_COUNT_BOUND: NumericBound = { field: 'count', min: 1, max: 20, integer: true }
const ALERT_MAX_ITEMS_BOUND: NumericBound = { field: 'max_items', min: 1, max: 50, integer: true }

const NUMERIC_BOUNDS: Partial<Record<string, NumericBound[]>> = {
  gauge: [GAUGE_MAX_BOUND],
  'top-n': [TOP_N_COUNT_BOUND],
  'alert-list': [ALERT_MAX_ITEMS_BOUND]
}

function isBoundValid(value: unknown, bound: NumericBound): boolean {
  if (value === undefined) {
    return true
  }
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return false
  }
  if (bound.integer && !Number.isInteger(value)) {
    return false
  }
  return value >= bound.min && value <= bound.max
}

function isConfigWithinBounds(widgetType: string, config: Record<string, unknown>): boolean {
  return (NUMERIC_BOUNDS[widgetType] ?? []).every((bound) => isBoundValid(config[bound.field], bound))
}

function BoundedNumberField({
  label,
  bound,
  value,
  onChange,
  placeholder,
  t
}: {
  bound: NumericBound
  label: string
  onChange: (value: number | undefined) => void
  placeholder: string
  t: (key: string, options?: Record<string, unknown>) => string
  value: number | undefined
}) {
  const valid = isBoundValid(value, bound)
  return (
    <div className="space-y-1.5">
      <Label>{label}</Label>
      <Input
        aria-invalid={!valid}
        max={bound.max}
        min={bound.min}
        onChange={(e) => onChange(e.target.value === '' ? undefined : Number(e.target.value))}
        placeholder={placeholder}
        type="number"
        value={value ?? ''}
      />
      {!valid && (
        <p className="text-destructive text-xs">
          {t('widgets.common.errors.numberRange', { min: bound.min, max: bound.max })}
        </p>
      )}
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

function MetricCardForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<MetricCardConfig>
  onChange: (c: Partial<MetricCardConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  const METRIC_CARD_METRICS = useMetricCardMetrics(t)
  const metric = (config.metric ?? 'cpu') as MetricCardMetric
  const serverId = config.server_id ?? ''
  const label = config.label ?? ''

  return (
    <>
      <ServerSelect
        label={t('widgets.common.labels.server')}
        onChange={(v) => onChange({ ...config, server_id: v })}
        placeholder={t('widgets.common.placeholders.selectServer')}
        servers={servers}
        value={serverId}
      />
      <MetricSelect
        label={t('widgets.common.labels.metric')}
        metrics={METRIC_CARD_METRICS}
        onChange={(v) => onChange({ ...config, metric: v as MetricCardMetric })}
        placeholder={t('widgets.common.placeholders.selectMetric')}
        value={metric}
      />
      <div className="space-y-1.5">
        <Label>{t('widgets.common.labels.labelOptional')}</Label>
        <Input
          onChange={(e) => onChange({ ...config, label: e.target.value })}
          placeholder={t('widgets.common.placeholders.optionalLabel')}
          value={label}
        />
      </div>
    </>
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
  t: (key: string, options?: Record<string, unknown>) => string
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
      <div className="space-y-1.5">
        <Label>{t('widgets.common.labels.labelOptional')}</Label>
        <Input
          onChange={(e) => onChange({ ...config, label: e.target.value || undefined })}
          placeholder={t('widgets.common.placeholders.optionalLabel')}
          value={config.label ?? ''}
        />
      </div>
      <BoundedNumberField
        bound={GAUGE_MAX_BOUND}
        label={t('widgets.common.labels.maxOptional')}
        onChange={(v) => onChange({ ...config, max: v })}
        placeholder="100"
        t={t}
        value={config.max}
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
      <CheckboxMultiSelect
        emptyMessage={t('widgets.common.empty.noServers')}
        items={servers}
        label={t('widgets.common.labels.servers')}
        onChange={(ids) => onChange({ ...config, server_ids: ids })}
        selected={config.server_ids ?? []}
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

function useSortOptions(t: (key: string) => string): { label: string; value: string }[] {
  return [
    { label: t('widgets.common.sort.desc'), value: 'desc' },
    { label: t('widgets.common.sort.asc'), value: 'asc' }
  ]
}

function TopNForm({
  config,
  onChange,
  t
}: {
  config: Partial<TopNConfig>
  onChange: (c: Partial<TopNConfig>) => void
  t: (key: string, options?: Record<string, unknown>) => string
}) {
  const TOP_N_METRICS = useTopNMetrics(t)
  const SORT_OPTIONS = useSortOptions(t)
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
        <Label>{t('widgets.common.labels.sortOrder')}</Label>
        <Select
          items={SORT_OPTIONS}
          onValueChange={(v) => v !== null && onChange({ ...config, sort: v as TopNConfig['sort'] })}
          value={config.sort ?? 'desc'}
        >
          <SelectTrigger className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {SORT_OPTIONS.map((o) => (
              <SelectItem key={o.value} value={o.value}>
                {o.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <BoundedNumberField
        bound={TOP_N_COUNT_BOUND}
        label={t('widgets.common.labels.count')}
        onChange={(v) => onChange({ ...config, count: v })}
        placeholder="5"
        t={t}
        value={config.count}
      />
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
  const layout: ServerCardsLayout = config.layout ?? 'grid'
  return (
    <>
      <div className="space-y-1.5">
        <Label>{t('widgets.common.labels.layout')}</Label>
        <ToggleGroup
          className="w-full"
          multiple={false}
          onValueChange={(value) => value.length > 0 && onChange({ ...config, layout: value[0] as ServerCardsLayout })}
          value={[layout]}
          variant="outline"
        >
          <ToggleGroupItem className="flex-1" value="grid">
            <LayoutGrid className="size-4" />
            {t('widgets.common.labels.layoutGrid')}
          </ToggleGroupItem>
          <ToggleGroupItem className="flex-1" value="list">
            <List className="size-4" />
            {t('widgets.common.labels.layoutList')}
          </ToggleGroupItem>
        </ToggleGroup>
      </div>
      <CheckboxMultiSelect
        emptyMessage={t('widgets.common.empty.noServers')}
        hint={t('widgets.common.hints.allServersWhenEmpty')}
        items={servers}
        label={t('widgets.common.labels.servers')}
        onChange={(ids) => onChange({ ...config, server_ids: ids })}
        selected={config.server_ids ?? []}
      />
    </>
  )
}

function useTrafficDaysOptions(t: (key: string) => string): { label: string; value: string }[] {
  // Traffic bars are daily aggregates; values are days expressed in hours so
  // they stay compatible with the stored `hours` config field.
  return [
    { label: t('common.timeRange.7days'), value: '168' },
    { label: t('common.timeRange.30days'), value: '720' },
    { label: t('common.timeRange.60days'), value: '1440' },
    { label: t('common.timeRange.90days'), value: '2160' }
  ]
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
  const TRAFFIC_DAYS_OPTIONS = useTrafficDaysOptions(t)
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
      <div className="space-y-1.5">
        <Label>{t('widgets.common.labels.timeRange')}</Label>
        <Select
          items={TRAFFIC_DAYS_OPTIONS}
          onValueChange={(v) => v !== null && onChange({ ...config, hours: Number(v) })}
          value={String(config.hours ?? '720')}
        >
          <SelectTrigger className="w-full">
            <SelectValue placeholder={t('widgets.common.placeholders.selectRange')} />
          </SelectTrigger>
          <SelectContent>
            {TRAFFIC_DAYS_OPTIONS.map((r) => (
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
  servers,
  onChange,
  t
}: {
  config: Partial<AlertListConfig>
  onChange: (c: Partial<AlertListConfig>) => void
  servers: ServerMetrics[]
  t: (key: string, options?: Record<string, unknown>) => string
}) {
  return (
    <>
      <CheckboxMultiSelect
        emptyMessage={t('widgets.common.empty.noServers')}
        hint={t('widgets.common.hints.allServersWhenEmpty')}
        items={servers}
        label={t('widgets.common.labels.servers')}
        onChange={(ids) => onChange({ ...config, server_ids: ids })}
        selected={config.server_ids ?? []}
      />
      <BoundedNumberField
        bound={ALERT_MAX_ITEMS_BOUND}
        label={t('widgets.common.labels.maxItems')}
        onChange={(v) => onChange({ ...config, max_items: v })}
        placeholder="10"
        t={t}
        value={config.max_items}
      />
    </>
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
  const contentId = useId()
  const content = config.content ?? ''

  return (
    <div className="space-y-1.5">
      <Label htmlFor={contentId}>{t('widgets.common.labels.markdownContent')}</Label>
      <textarea
        aria-label={t('widgets.common.labels.markdownContent')}
        className="h-32 w-full rounded-lg border border-input bg-transparent px-2.5 py-2 text-sm outline-none transition-colors focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30"
        id={contentId}
        onChange={(e) => onChange({ ...config, content: e.target.value })}
        placeholder={t('widgets.common.placeholders.writeMarkdown')}
        value={config.content ?? ''}
      />
      {content.length > 0 && (
        <div className="rounded-lg border p-3">
          <p className="mb-1 font-medium text-muted-foreground text-xs">{t('dialogs.widgetConfig.labels.preview')}</p>
          <ScrollArea className="max-h-32">
            <MarkdownContent className="prose prose-sm dark:prose-invert max-w-none text-sm" content={content} />
          </ScrollArea>
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

function ModuleForm({
  entry,
  config,
  onChange,
  t
}: {
  config: Record<string, unknown>
  entry: RegistryEntry
  onChange: (c: Record<string, unknown>) => void
  t: (key: string) => string
}) {
  const schema = entry.module.configSchema
  const info = useMemo(() => schema.introspect(), [schema])
  if (info.kind !== 'object' || !info.shape || Object.keys(info.shape).length === 0) {
    return <p className="text-muted-foreground text-sm">{t('module_config_no_fields')}</p>
  }
  return (
    <div data-testid="module-config-form">
      <ConfigForm onChange={onChange} schema={schema} value={config} />
    </div>
  )
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
      <CheckboxMultiSelect
        emptyMessage={t('widgets.common.empty.noServers')}
        hint={t('widgets.common.hints.allServersWhenEmpty')}
        items={servers}
        label={t('widgets.common.labels.servers')}
        onChange={(ids) => onChange({ ...config, server_ids: ids })}
        selected={config.server_ids ?? []}
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

function NetworkLatencyForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<NetworkLatencyConfig>
  onChange: (c: Partial<NetworkLatencyConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  const NETWORK_RANGE_OPTIONS = useNetworkRangeOptions(t)
  return (
    <>
      <ServerSelect
        label={t('widgets.common.labels.server')}
        onChange={(v) => onChange({ ...config, server_id: v })}
        placeholder={t('widgets.common.placeholders.selectServer')}
        servers={servers}
        value={config.server_id ?? ''}
      />
      <div className="space-y-1.5">
        <Label>{t('widgets.common.labels.timeRange')}</Label>
        <Select
          items={NETWORK_RANGE_OPTIONS}
          onValueChange={(v) => v !== null && onChange({ ...config, hours: Number(v) })}
          value={String(config.hours ?? '24')}
        >
          <SelectTrigger className="w-full">
            <SelectValue placeholder={t('widgets.common.placeholders.selectRange')} />
          </SelectTrigger>
          <SelectContent>
            {NETWORK_RANGE_OPTIONS.map((r) => (
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

function NetworkQualityForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<NetworkQualityConfig>
  onChange: (c: Partial<NetworkQualityConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  return (
    <ServerSelect
      label={t('widgets.common.labels.server')}
      onChange={(v) => onChange({ ...config, server_id: v })}
      placeholder={t('widgets.common.placeholders.selectServer')}
      servers={servers}
      value={config.server_id ?? ''}
    />
  )
}

function NetworkOverviewForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<NetworkOverviewConfig>
  onChange: (c: Partial<NetworkOverviewConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  return (
    <CheckboxMultiSelect
      emptyMessage={t('widgets.common.empty.noServers')}
      hint={t('widgets.common.hints.allServersWhenEmpty')}
      items={servers}
      label={t('widgets.common.labels.servers')}
      onChange={(ids) => onChange({ ...config, server_ids: ids })}
      selected={config.server_ids ?? []}
    />
  )
}

function ServerMapForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<ServerMapConfig>
  onChange: (c: Partial<ServerMapConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  return (
    <CheckboxMultiSelect
      emptyMessage={t('widgets.common.empty.noServers')}
      hint={t('widgets.common.hints.allServersWhenEmpty')}
      items={servers}
      label={t('widgets.common.labels.servers')}
      onChange={(ids) => onChange({ ...config, server_ids: ids })}
      selected={config.server_ids ?? []}
    />
  )
}

function ServiceStatusForm({
  config,
  onChange,
  t
}: {
  config: Partial<ServiceStatusConfig>
  onChange: (c: Partial<ServiceStatusConfig>) => void
  t: (key: string) => string
}) {
  const { data: monitors } = useQuery<{ id: string; name: string }[]>({
    queryKey: ['service-monitors'],
    queryFn: () => api.get<{ id: string; name: string }[]>('/api/service-monitors')
  })
  return (
    <CheckboxMultiSelect
      emptyMessage={t('widgets.serviceStatus.empty.noMonitors')}
      hint={t('widgets.common.hints.allMonitorsWhenEmpty')}
      items={monitors ?? []}
      label={t('widgets.common.labels.monitors')}
      onChange={(ids) => onChange({ ...config, monitor_ids: ids })}
      selected={config.monitor_ids ?? []}
    />
  )
}

export function WidgetConfigDialog({ widget, widgetType, ...props }: WidgetConfigDialogProps) {
  const formKey = widget ? `edit:${widget.id}` : `new:${widgetType}`

  return <WidgetConfigDialogForm key={formKey} widget={widget} widgetType={widgetType} {...props} />
}

// biome-ignore lint/complexity/noExcessiveCognitiveComplexity: dispatcher renders one form per widget_type; refactoring to a table is more work than the value
function WidgetConfigDialogForm({
  open,
  onOpenChange,
  onSubmit,
  widgetType,
  widget,
  servers
}: WidgetConfigDialogProps) {
  const { t } = useTranslation('dashboard')

  const [draft, dispatchDraft] = useReducer(widgetFormReducer, widget, createWidgetFormDraft)
  const setConfig = (config: object) => dispatchDraft({ type: 'configChanged', config })

  const isModule = widgetType === 'module'
  const moduleId = widget?.module_id ?? null
  const moduleEntry = useMemo(
    () => (isModule && moduleId ? registryActions.get(moduleId) : undefined),
    [isModule, moduleId]
  )
  const moduleMissing = isModule && !moduleEntry
  const configOutOfBounds = !isConfigWithinBounds(widgetType, draft.config)

  const handleSubmit = () => {
    // Seed per-widget defaults that the form only shows but doesn't write,
    // so a "save without changes" doesn't persist an invalid empty config
    // (e.g. the metric-card form displays 'cpu' but only commits it on user change).
    const normalized = { ...draft.config }
    if (widgetType === 'metric-card' && normalized.metric === undefined) {
      normalized.metric = 'cpu'
    }
    onSubmit(draft.title, JSON.stringify(normalized))
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
          {/* Only module (custom) widgets render widget.title; built-in widgets
              draw their own contextual headers and ignore it, so offering the
              field there is a dead control. */}
          {isModule && (
            <div className="space-y-1.5">
              <Label>{t('dialogs.widgetConfig.labels.titleOptional')}</Label>
              <Input
                onChange={(e) => dispatchDraft({ type: 'titleChanged', value: e.target.value })}
                placeholder={t('dialogs.widgetConfig.placeholders.widgetTitle')}
                value={draft.title}
              />
            </div>
          )}

          {widgetType === 'stat-number' && (
            <StatNumberForm config={draft.config as Partial<StatNumberConfig>} onChange={setConfig} t={t} />
          )}
          {widgetType === 'metric-card' && (
            <MetricCardForm
              config={draft.config as Partial<MetricCardConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {widgetType === 'gauge' && (
            <GaugeForm config={draft.config as Partial<GaugeConfig>} onChange={setConfig} servers={servers} t={t} />
          )}
          {widgetType === 'line-chart' && (
            <LineChartForm
              config={draft.config as Partial<LineChartConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {widgetType === 'multi-line' && (
            <MultiLineForm
              config={draft.config as Partial<MultiLineConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {widgetType === 'top-n' && (
            <TopNForm config={draft.config as Partial<TopNConfig>} onChange={setConfig} t={t} />
          )}
          {widgetType === 'server-cards' && (
            <ServerCardsForm
              config={draft.config as Partial<ServerCardsConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {widgetType === 'traffic-bar' && (
            <TrafficBarForm
              config={draft.config as Partial<TrafficBarConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {widgetType === 'disk-io' && (
            <DiskIoForm config={draft.config as Partial<DiskIoConfig>} onChange={setConfig} servers={servers} t={t} />
          )}
          {widgetType === 'alert-list' && (
            <AlertListForm
              config={draft.config as Partial<AlertListConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {widgetType === 'service-status' && (
            <ServiceStatusForm config={draft.config as Partial<ServiceStatusConfig>} onChange={setConfig} t={t} />
          )}
          {widgetType === 'server-map' && (
            <ServerMapForm
              config={draft.config as Partial<ServerMapConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {widgetType === 'markdown' && (
            <MarkdownForm config={draft.config as Partial<MarkdownConfig>} onChange={setConfig} t={t} />
          )}
          {widgetType === 'uptime-timeline' && (
            <UptimeTimelineForm
              config={draft.config as Partial<UptimeTimelineConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {widgetType === 'network-latency' && (
            <NetworkLatencyForm
              config={draft.config as Partial<NetworkLatencyConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {widgetType === 'network-quality' && (
            <NetworkQualityForm
              config={draft.config as Partial<NetworkQualityConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {widgetType === 'network-overview' && (
            <NetworkOverviewForm
              config={draft.config as Partial<NetworkOverviewConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {isModule && moduleEntry && (
            <ModuleForm config={draft.config} entry={moduleEntry} onChange={setConfig} t={t} />
          )}
          {moduleMissing && (
            <p className="text-destructive text-sm">
              {moduleId ? t('module_not_installed_id').replace('{{id}}', moduleId) : t('module_not_installed')}
            </p>
          )}
        </div>
        <DialogFooter>
          <Button disabled={moduleMissing || configOutOfBounds} onClick={handleSubmit}>
            {widget ? t('save') : t('add_widget')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
