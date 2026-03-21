import {
  Activity,
  BarChart3,
  FileText,
  Gauge,
  Globe,
  HardDrive,
  LayoutGrid,
  LineChart,
  List,
  Network,
  Server,
  TrendingUp
} from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { WIDGET_TYPES, type WidgetCategory } from '@/lib/widget-types'

interface WidgetPickerProps {
  onOpenChange: (open: boolean) => void
  onSelect: (widgetType: string) => void
  open: boolean
}

const WIDGET_ICONS: Record<string, typeof Server> = {
  'stat-number': TrendingUp,
  'server-cards': LayoutGrid,
  gauge: Gauge,
  'line-chart': LineChart,
  'multi-line': TrendingUp,
  'top-n': BarChart3,
  'alert-list': List,
  'service-status': Activity,
  'traffic-bar': Network,
  'disk-io': HardDrive,
  'server-map': Globe,
  markdown: FileText,
  'uptime-timeline': Activity
}

const WIDGET_DESCRIPTIONS: Record<string, string> = {
  'stat-number': 'Single metric value with icon',
  'server-cards': 'Server overview cards grid',
  gauge: 'Circular gauge for a metric',
  'line-chart': 'Time series line chart',
  'multi-line': 'Compare metrics across servers',
  'top-n': 'Ranked list of servers',
  'alert-list': 'Recent alert events',
  'service-status': 'Service monitor status dots',
  'traffic-bar': 'Daily traffic bar chart',
  'disk-io': 'Disk I/O read/write chart',
  'server-map': 'World map with server locations',
  markdown: 'Custom markdown content',
  'uptime-timeline': '90-day uptime timeline bar'
}

const CATEGORY_ORDER: WidgetCategory[] = ['Real-time', 'Charts', 'Status']

export function WidgetPicker({ onSelect, open, onOpenChange }: WidgetPickerProps) {
  const { t } = useTranslation('dashboard')

  const grouped = useMemo(() => {
    const map = new Map<WidgetCategory, typeof WIDGET_TYPES>()
    for (const category of CATEGORY_ORDER) {
      map.set(
        category,
        WIDGET_TYPES.filter((w) => w.category === category)
      )
    }
    return map
  }, [])

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('add_widget', 'Add Widget')}</DialogTitle>
        </DialogHeader>
        <div className="max-h-96 space-y-4 overflow-y-auto">
          {CATEGORY_ORDER.map((category) => {
            const widgets = grouped.get(category)
            if (!widgets || widgets.length === 0) {
              return null
            }
            return (
              <div key={category}>
                <h4 className="mb-2 font-medium text-muted-foreground text-xs uppercase tracking-wide">{category}</h4>
                <div className="grid grid-cols-2 gap-2">
                  {widgets.map((widgetType) => {
                    const Icon = WIDGET_ICONS[widgetType.id] ?? Server
                    const description = WIDGET_DESCRIPTIONS[widgetType.id] ?? ''
                    return (
                      <button
                        className="flex items-start gap-3 rounded-lg border bg-card p-3 text-left transition-colors hover:bg-muted/50"
                        key={widgetType.id}
                        onClick={() => {
                          onSelect(widgetType.id)
                          onOpenChange(false)
                        }}
                        type="button"
                      >
                        <div className="rounded-md bg-muted p-1.5">
                          <Icon className="size-4 text-muted-foreground" />
                        </div>
                        <div className="min-w-0">
                          <p className="font-medium text-sm leading-tight">{widgetType.label}</p>
                          <p className="mt-0.5 text-muted-foreground text-xs leading-snug">{description}</p>
                        </div>
                      </button>
                    )
                  })}
                </div>
              </div>
            )
          })}
        </div>
      </DialogContent>
    </Dialog>
  )
}
