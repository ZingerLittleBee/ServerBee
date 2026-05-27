import {
  Activity,
  BarChart3,
  Cpu,
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
import { ScrollArea } from '@/components/ui/scroll-area'
import { WIDGET_TYPES, type WidgetCategory } from '@/lib/widget-types'

interface WidgetPickerProps {
  onOpenChange: (open: boolean) => void
  onSelect: (widgetType: string) => void
  open: boolean
}

const WIDGET_ICONS: Record<string, typeof Server> = {
  'stat-number': TrendingUp,
  'metric-card': Cpu,
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

const CATEGORY_ORDER: WidgetCategory[] = ['Real-time', 'Charts', 'Status']

export function WidgetPicker({ onSelect, open, onOpenChange }: WidgetPickerProps) {
  const { t } = useTranslation('dashboard')

  const grouped = useMemo(() => {
    const map = new Map<WidgetCategory, (typeof WIDGET_TYPES)[number][]>()
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
      <DialogContent className="sm:max-h-[80vh] sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('add_widget', 'Add Widget')}</DialogTitle>
        </DialogHeader>
        <ScrollArea className="max-h-[60vh]">
          <div className="space-y-4 pr-3">
            {CATEGORY_ORDER.map((category) => {
              const widgets = grouped.get(category)
              if (!widgets || widgets.length === 0) {
                return null
              }
              return (
                <div key={category}>
                  <h4 className="mb-2 font-medium text-muted-foreground text-xs uppercase tracking-wide">
                    {t(`widgetPicker.categories.${category}`, category)}
                  </h4>
                  <div className="grid gap-2 sm:grid-cols-2">
                    {widgets.map((widgetType) => {
                      const Icon = WIDGET_ICONS[widgetType.id] ?? Server
                      const label = t(`widgetPicker.types.${widgetType.id}.label`, widgetType.label)
                      const description = t(`widgetPicker.types.${widgetType.id}.description`, '')
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
                            <p className="font-medium text-sm leading-tight">{label}</p>
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
        </ScrollArea>
      </DialogContent>
    </Dialog>
  )
}
