import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertTriangle, ExternalLink } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'
import { api } from '@/lib/api-client'
import type { ServerResponse, StatusPageItem, UpdateStatusPageRequest } from '@/lib/api-schema'
import { buildStatusPageUpdatePayload, type ConfigFormState, configFromItem } from './status-page-config-utils'
import { StatusPagePanelToggle } from './status-page-panel-toggle'
import { StatusPageServerCheckboxItem } from './status-page-server-checkbox-item'

export function StatusPageConfigForm({ servers }: { servers: ServerResponse[] }) {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()

  const { data: config, isLoading } = useQuery<StatusPageItem>({
    queryKey: ['status-page-config'],
    queryFn: () => api.get<StatusPageItem>('/api/status-page')
  })

  const [state, setState] = useState<ConfigFormState | null>(null)

  if (config && state === null) {
    setState(configFromItem(config))
  }

  const mutation = useMutation({
    mutationFn: (input: UpdateStatusPageRequest) => api.put<StatusPageItem>('/api/status-page', input),
    onSuccess: (next) => {
      queryClient.setQueryData(['status-page-config'], next)
      queryClient.invalidateQueries({ queryKey: ['status-page-config'] }).catch(() => undefined)
      toast.success(t('status_pages.config_saved'))
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t('common:errors.failed'))
  })

  if (isLoading || !state) {
    return (
      <div className="space-y-2">
        {Array.from({ length: 4 }, (_, i) => (
          <Skeleton className="h-12" key={`skel-${i.toString()}`} />
        ))}
      </div>
    )
  }

  const update = <K extends keyof ConfigFormState>(key: K, value: ConfigFormState[K]) => {
    setState((prev) => (prev ? { ...prev, [key]: value } : prev))
  }

  const toggleServer = (id: string) => {
    setState((prev) => {
      if (!prev) {
        return prev
      }
      const next = prev.selectedServers.includes(id)
        ? prev.selectedServers.filter((serverId) => serverId !== id)
        : [...prev.selectedServers, id]
      return { ...prev, selectedServers: next }
    })
  }

  const handleSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!state.title.trim()) {
      toast.error(t('status_pages.title_required'))
      return
    }
    mutation.mutate(buildStatusPageUpdatePayload(state))
  }

  return (
    <form className="space-y-6" onSubmit={handleSubmit}>
      <Card>
        <CardHeader>
          <CardTitle>{t('status_pages.section_general')}</CardTitle>
          <CardDescription>
            <a
              className="inline-flex items-center gap-1 font-mono text-primary text-xs hover:underline"
              href="/status"
              rel="noopener noreferrer"
              target="_blank"
            >
              /status
              <ExternalLink className="size-3" />
            </a>
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {!state.enabled && (
            <div className="flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-amber-700 text-sm dark:text-amber-300">
              <AlertTriangle className="mt-0.5 size-4 shrink-0" />
              <p>{t('status_pages.site_disabled_notice_admin')}</p>
            </div>
          )}

          <div className="flex items-center justify-between gap-4">
            <div className="space-y-0.5">
              <Label htmlFor="sp-enabled">{t('status_pages.field_enabled')}</Label>
              <p className="text-muted-foreground text-xs">{t('status_pages.field_enabled_hint')}</p>
            </div>
            <Switch checked={state.enabled} id="sp-enabled" onCheckedChange={(value) => update('enabled', value)} />
          </div>

          <div className="space-y-1">
            <Label htmlFor="sp-title">{t('status_pages.field_title')}</Label>
            <Input
              id="sp-title"
              onChange={(e) => update('title', e.target.value)}
              placeholder={t('status_pages.placeholder_title')}
              required
              value={state.title}
            />
          </div>

          <div className="space-y-1">
            <Label htmlFor="sp-desc">{t('status_pages.field_description')}</Label>
            <Textarea
              id="sp-desc"
              onChange={(e) => update('description', e.target.value)}
              placeholder={t('status_pages.placeholder_description')}
              rows={2}
              value={state.description}
            />
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('status_pages.section_servers')}</CardTitle>
          <CardDescription>{t('status_pages.section_servers_description')}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-1">
            <Label htmlFor="sp-layout">{t('status_pages.field_default_layout')}</Label>
            <Select
              items={{
                list: t('status_pages.layout_list'),
                grid: t('status_pages.layout_grid')
              }}
              onValueChange={(value) => {
                if (value === 'list' || value === 'grid') {
                  update('defaultLayout', value)
                }
              }}
              value={state.defaultLayout}
            >
              <SelectTrigger id="sp-layout">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="list">{t('status_pages.layout_list')}</SelectItem>
                <SelectItem value="grid">{t('status_pages.layout_grid')}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <Label>{t('status_pages.field_servers')}</Label>
            <ScrollArea className="h-40 rounded-md border">
              <div className="space-y-1 p-2">
                {servers.map((server) => (
                  <StatusPageServerCheckboxItem
                    checked={state.selectedServers.includes(server.id)}
                    key={server.id}
                    name={server.name}
                    onToggle={() => toggleServer(server.id)}
                  />
                ))}
                {servers.length === 0 && (
                  <p className="text-muted-foreground text-xs">{t('status_pages.no_servers')}</p>
                )}
              </div>
            </ScrollArea>
            <p className="text-muted-foreground text-xs">{t('status_pages.field_servers_hint')}</p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('status_pages.section_panels')}</CardTitle>
          <CardDescription>{t('status_pages.section_panels_description')}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <StatusPagePanelToggle
            checked={state.showServerDetail}
            description={t('status_pages.field_show_server_detail_hint')}
            id="sp-show-server-detail"
            label={t('status_pages.field_show_server_detail')}
            onChange={(value) => update('showServerDetail', value)}
          />
          <StatusPagePanelToggle
            checked={state.showNetwork}
            description={t('status_pages.field_show_network_hint')}
            id="sp-show-network"
            label={t('status_pages.field_show_network')}
            onChange={(value) => update('showNetwork', value)}
          />
          <StatusPagePanelToggle
            checked={state.showIpQuality}
            description={t('status_pages.field_show_ip_quality_hint')}
            id="sp-show-ip-quality"
            label={t('status_pages.field_show_ip_quality')}
            onChange={(value) => update('showIpQuality', value)}
          />
          <StatusPagePanelToggle
            checked={state.showIncidents}
            description={t('status_pages.field_show_incidents_hint')}
            id="sp-show-incidents"
            label={t('status_pages.field_show_incidents')}
            onChange={(value) => update('showIncidents', value)}
          />
          <StatusPagePanelToggle
            checked={state.showMaintenance}
            description={t('status_pages.field_show_maintenance_hint')}
            id="sp-show-maintenance"
            label={t('status_pages.field_show_maintenance')}
            onChange={(value) => update('showMaintenance', value)}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('status_pages.section_thresholds')}</CardTitle>
          <CardDescription>{t('status_pages.section_thresholds_description')}</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <Label htmlFor="sp-yellow">{t('status_pages.uptime_yellow_label')}</Label>
              <Input
                id="sp-yellow"
                max={100}
                min={0}
                onChange={(e) => update('yellowThreshold', Number(e.target.value) || 100)}
                step={0.1}
                type="number"
                value={state.yellowThreshold}
              />
              <p className="text-muted-foreground text-xs">{t('status_pages.uptime_yellow_hint')}</p>
            </div>
            <div className="space-y-1">
              <Label htmlFor="sp-red">{t('status_pages.uptime_red_label')}</Label>
              <Input
                id="sp-red"
                max={100}
                min={0}
                onChange={(e) => update('redThreshold', Number(e.target.value) || 95)}
                step={0.1}
                type="number"
                value={state.redThreshold}
              />
              <p className="text-muted-foreground text-xs">{t('status_pages.uptime_red_hint')}</p>
            </div>
          </div>
        </CardContent>
      </Card>

      <div className="flex justify-end">
        <Button disabled={mutation.isPending} type="submit">
          {t('common:save')}
        </Button>
      </div>
    </form>
  )
}
