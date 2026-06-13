import type { ActionsHelper } from '@serverbee/widget-sdk'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { ScrollArea } from '@/components/ui/scroll-area'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { parseConfig } from '@/lib/widget-helpers'
import type { DashboardWidget } from '@/lib/widget-types'
import { useWidgetRegistry } from '@/widgets-runtime/registry'

interface ModuleWidgetHostProps {
  servers: ServerMetrics[]
  widget: DashboardWidget
}

const NOOP_ACTIONS: ActionsHelper = {
  render: () => null
}

function Placeholder({ message }: { message: string }) {
  return (
    <div className="flex h-full items-center justify-center rounded-lg border border-dashed bg-card p-4 text-center text-muted-foreground text-sm">
      {message}
    </div>
  )
}

export function ModuleWidgetHost({ widget, servers: _servers }: ModuleWidgetHostProps) {
  const { t } = useTranslation('dashboard')
  const moduleId = widget.module_id ?? ''
  const entry = useWidgetRegistry((state) => (moduleId ? state.modules.get(moduleId) : undefined))

  const parsed = useMemo(() => {
    if (!entry) {
      return { ok: false as const, error: '' }
    }
    const raw = parseConfig<unknown>(widget.config_json)
    try {
      const config = entry.module.configSchema.parse(raw)
      return { ok: true as const, config }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      return { ok: false as const, error: message }
    }
  }, [entry, widget.config_json])

  if (!moduleId) {
    return <Placeholder message={t('module_not_installed', 'Widget module not installed')} />
  }
  if (!entry) {
    return (
      <Placeholder message={t('module_not_installed_id', 'Widget module "{{id}}" not installed', { id: moduleId })} />
    )
  }
  if (!parsed.ok) {
    return (
      <Placeholder
        message={t('module_config_invalid', 'Widget config invalid: {{message}}', { message: parsed.error })}
      />
    )
  }

  const Component = entry.module.component
  const title = widget.title || entry.manifest.name
  return (
    <div className="flex h-full min-w-0 flex-col rounded-lg border bg-card p-4 shadow-sm">
      <div className="mb-3 min-w-0 truncate font-semibold text-sm">{title}</div>
      <ScrollArea className="min-h-0 flex-1">
        <Component
          actions={NOOP_ACTIONS}
          config={parsed.config}
          isEditing={false}
          size={{ w: widget.grid_w, h: widget.grid_h }}
        />
      </ScrollArea>
    </div>
  )
}
