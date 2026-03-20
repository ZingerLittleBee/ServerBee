import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { renderMarkdown } from '@/lib/markdown'
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
const STAT_METRICS = [
  { label: 'Server Count', value: 'server_count' },
  { label: 'Average CPU', value: 'avg_cpu' },
  { label: 'Average Memory', value: 'avg_memory' },
  { label: 'Total Bandwidth', value: 'total_bandwidth' },
  { label: 'Health', value: 'health' }
]

// Metric options for gauge / line-chart / multi-line
const GAUGE_METRICS = [
  { label: 'CPU', value: 'cpu' },
  { label: 'Memory', value: 'memory' },
  { label: 'Disk', value: 'disk' },
  { label: 'Swap', value: 'swap' }
]

const LINE_METRICS = [
  { label: 'CPU', value: 'cpu' },
  { label: 'Memory', value: 'memory' },
  { label: 'Disk', value: 'disk' },
  { label: 'Load (1m)', value: 'load1' },
  { label: 'Load (5m)', value: 'load5' },
  { label: 'Load (15m)', value: 'load15' },
  { label: 'Network In', value: 'net_in' },
  { label: 'Network Out', value: 'net_out' }
]

const TOP_N_METRICS = [
  { label: 'CPU', value: 'cpu' },
  { label: 'Memory', value: 'memory' },
  { label: 'Disk', value: 'disk' },
  { label: 'Bandwidth', value: 'bandwidth' }
]

const RANGE_OPTIONS = [
  { label: '1 hour', value: '1' },
  { label: '6 hours', value: '6' },
  { label: '12 hours', value: '12' },
  { label: '24 hours', value: '24' },
  { label: '3 days', value: '72' },
  { label: '7 days', value: '168' }
]

function parseExistingConfig(widget?: DashboardWidget): WidgetConfig | null {
  if (!widget) {
    return null
  }
  try {
    return JSON.parse(widget.config_json) as WidgetConfig
  } catch {
    return null
  }
}

function ServerSelect({
  label,
  servers,
  value,
  onChange
}: {
  label: string
  onChange: (v: string) => void
  servers: ServerMetrics[]
  value: string
}) {
  return (
    <div className="space-y-1.5">
      <Label>{label}</Label>
      <Select onValueChange={(v) => v !== null && onChange(v)} value={value}>
        <SelectTrigger className="w-full">
          <SelectValue placeholder="Select server" />
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
  onChange
}: {
  label: string
  metrics: { label: string; value: string }[]
  onChange: (v: string) => void
  value: string
}) {
  return (
    <div className="space-y-1.5">
      <Label>{label}</Label>
      <Select onValueChange={(v) => v !== null && onChange(v)} value={value}>
        <SelectTrigger className="w-full">
          <SelectValue placeholder="Select metric" />
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

function RangeSelect({ value, onChange }: { onChange: (v: string) => void; value: string }) {
  return (
    <div className="space-y-1.5">
      <Label>Time Range</Label>
      <Select onValueChange={(v) => v !== null && onChange(v)} value={value}>
        <SelectTrigger className="w-full">
          <SelectValue placeholder="Select range" />
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
  onChange
}: {
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
        {servers.length === 0 && <p className="py-2 text-center text-muted-foreground text-xs">No servers available</p>}
      </div>
    </div>
  )
}

// Individual config forms per widget type

function StatNumberForm({
  config,
  onChange
}: {
  config: Partial<StatNumberConfig>
  onChange: (c: Partial<StatNumberConfig>) => void
}) {
  return (
    <MetricSelect
      label="Metric"
      metrics={STAT_METRICS}
      onChange={(v) => onChange({ ...config, metric: v })}
      value={config.metric ?? ''}
    />
  )
}

function GaugeForm({
  config,
  servers,
  onChange
}: {
  config: Partial<GaugeConfig>
  onChange: (c: Partial<GaugeConfig>) => void
  servers: ServerMetrics[]
}) {
  return (
    <>
      <ServerSelect
        label="Server"
        onChange={(v) => onChange({ ...config, server_id: v })}
        servers={servers}
        value={config.server_id ?? ''}
      />
      <MetricSelect
        label="Metric"
        metrics={GAUGE_METRICS}
        onChange={(v) => onChange({ ...config, metric: v })}
        value={config.metric ?? ''}
      />
    </>
  )
}

function LineChartForm({
  config,
  servers,
  onChange
}: {
  config: Partial<LineChartConfig>
  onChange: (c: Partial<LineChartConfig>) => void
  servers: ServerMetrics[]
}) {
  return (
    <>
      <ServerSelect
        label="Server"
        onChange={(v) => onChange({ ...config, server_id: v })}
        servers={servers}
        value={config.server_id ?? ''}
      />
      <MetricSelect
        label="Metric"
        metrics={LINE_METRICS}
        onChange={(v) => onChange({ ...config, metric: v })}
        value={config.metric ?? ''}
      />
      <RangeSelect onChange={(v) => onChange({ ...config, hours: Number(v) })} value={String(config.hours ?? '24')} />
    </>
  )
}

function MultiLineForm({
  config,
  servers,
  onChange
}: {
  config: Partial<MultiLineConfig>
  onChange: (c: Partial<MultiLineConfig>) => void
  servers: ServerMetrics[]
}) {
  return (
    <>
      <ServerMultiSelect
        label="Servers"
        onChange={(ids) => onChange({ ...config, server_ids: ids })}
        selected={config.server_ids ?? []}
        servers={servers}
      />
      <MetricSelect
        label="Metric"
        metrics={LINE_METRICS}
        onChange={(v) => onChange({ ...config, metric: v })}
        value={config.metric ?? ''}
      />
      <RangeSelect onChange={(v) => onChange({ ...config, hours: Number(v) })} value={String(config.hours ?? '24')} />
    </>
  )
}

function TopNForm({ config, onChange }: { config: Partial<TopNConfig>; onChange: (c: Partial<TopNConfig>) => void }) {
  return (
    <>
      <MetricSelect
        label="Metric"
        metrics={TOP_N_METRICS}
        onChange={(v) => onChange({ ...config, metric: v })}
        value={config.metric ?? ''}
      />
      <div className="space-y-1.5">
        <Label>Count</Label>
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
  onChange
}: {
  config: Partial<ServerCardsConfig>
  onChange: (c: Partial<ServerCardsConfig>) => void
  servers: ServerMetrics[]
}) {
  return (
    <ServerMultiSelect
      label="Servers (optional, leave empty for all)"
      onChange={(ids) => onChange({ ...config, server_ids: ids })}
      selected={config.server_ids ?? []}
      servers={servers}
    />
  )
}

function TrafficBarForm({
  config,
  servers,
  onChange
}: {
  config: Partial<TrafficBarConfig>
  onChange: (c: Partial<TrafficBarConfig>) => void
  servers: ServerMetrics[]
}) {
  return (
    <>
      <div className="space-y-1.5">
        <Label>Server (optional, leave empty for global)</Label>
        <Select
          onValueChange={(v) => onChange({ ...config, server_id: v === '__all__' ? '' : (v ?? '') })}
          value={config.server_id || '__all__'}
        >
          <SelectTrigger className="w-full">
            <SelectValue placeholder="All servers" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">All Servers</SelectItem>
            {servers.map((s) => (
              <SelectItem key={s.id} value={s.id}>
                {s.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <RangeSelect onChange={(v) => onChange({ ...config, hours: Number(v) })} value={String(config.hours ?? '720')} />
    </>
  )
}

function DiskIoForm({
  config,
  servers,
  onChange
}: {
  config: Partial<DiskIoConfig>
  onChange: (c: Partial<DiskIoConfig>) => void
  servers: ServerMetrics[]
}) {
  return (
    <>
      <ServerSelect
        label="Server"
        onChange={(v) => onChange({ ...config, server_id: v })}
        servers={servers}
        value={config.server_id ?? ''}
      />
      <RangeSelect onChange={(v) => onChange({ ...config, hours: Number(v) })} value={String(config.hours ?? '24')} />
    </>
  )
}

function AlertListForm({
  config,
  onChange
}: {
  config: Partial<AlertListConfig>
  onChange: (c: Partial<AlertListConfig>) => void
}) {
  return (
    <div className="space-y-1.5">
      <Label>Max Items</Label>
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
  onChange
}: {
  config: Partial<MarkdownConfig>
  onChange: (c: Partial<MarkdownConfig>) => void
}) {
  const html = useMemo(() => renderMarkdown(config.content ?? ''), [config.content])

  return (
    <div className="space-y-1.5">
      <Label>Markdown Content</Label>
      <textarea
        className="h-32 w-full rounded-lg border border-input bg-transparent px-2.5 py-2 text-sm outline-none transition-colors focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30"
        onChange={(e) => onChange({ ...config, content: e.target.value })}
        placeholder="Write markdown here..."
        value={config.content ?? ''}
      />
      {(config.content ?? '').length > 0 && (
        <div className="rounded-lg border p-3">
          <p className="mb-1 font-medium text-muted-foreground text-xs">Preview</p>
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

export function WidgetConfigDialog({
  open,
  onOpenChange,
  onSubmit,
  widgetType,
  widget,
  servers
}: WidgetConfigDialogProps) {
  const { t } = useTranslation('dashboard')
  const existingConfig = parseExistingConfig(widget)

  const [title, setTitle] = useState(widget?.title ?? '')
  const [config, setConfig] = useState<Record<string, unknown>>(() => (existingConfig as Record<string, unknown>) ?? {})

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
            {widget ? t('edit_widget', 'Edit Widget') : t('configure_widget', 'Configure Widget')}
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-4">
          <div className="space-y-1.5">
            <Label>{t('widget_title', 'Title (optional)')}</Label>
            <Input onChange={(e) => setTitle(e.target.value)} placeholder="Widget title" value={title} />
          </div>

          {widgetType === 'stat-number' && (
            <StatNumberForm config={config as Partial<StatNumberConfig>} onChange={setConfig} />
          )}
          {widgetType === 'gauge' && (
            <GaugeForm config={config as Partial<GaugeConfig>} onChange={setConfig} servers={servers} />
          )}
          {widgetType === 'line-chart' && (
            <LineChartForm config={config as Partial<LineChartConfig>} onChange={setConfig} servers={servers} />
          )}
          {widgetType === 'multi-line' && (
            <MultiLineForm config={config as Partial<MultiLineConfig>} onChange={setConfig} servers={servers} />
          )}
          {widgetType === 'top-n' && <TopNForm config={config as Partial<TopNConfig>} onChange={setConfig} />}
          {widgetType === 'server-cards' && (
            <ServerCardsForm config={config as Partial<ServerCardsConfig>} onChange={setConfig} servers={servers} />
          )}
          {widgetType === 'traffic-bar' && (
            <TrafficBarForm config={config as Partial<TrafficBarConfig>} onChange={setConfig} servers={servers} />
          )}
          {widgetType === 'disk-io' && (
            <DiskIoForm config={config as Partial<DiskIoConfig>} onChange={setConfig} servers={servers} />
          )}
          {widgetType === 'alert-list' && (
            <AlertListForm config={config as Partial<AlertListConfig>} onChange={setConfig} />
          )}
          {widgetType === 'markdown' && (
            <MarkdownForm config={config as Partial<MarkdownConfig>} onChange={setConfig} />
          )}
          {needsNoConfig && <p className="text-muted-foreground text-sm">No additional configuration needed.</p>}
        </div>
        <DialogFooter>
          <Button onClick={handleSubmit}>{widget ? t('save', 'Save') : t('add', 'Add')}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
