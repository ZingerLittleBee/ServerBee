import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { api } from '@/lib/api-client'
import type { ServerResponse } from '@/lib/api-schema'
import { StatusPageConfigForm } from './status-page-config-form'
import { StatusPageIncidentsTab } from './status-page-incidents-tab'
import { StatusPageMaintenanceTab } from './status-page-maintenance-tab'

const STATUS_PAGE_TABS = ['config', 'incidents', 'maintenance'] as const
type StatusPageTab = (typeof STATUS_PAGE_TABS)[number]

function isStatusPageTab(value: unknown): value is StatusPageTab {
  return typeof value === 'string' && STATUS_PAGE_TABS.some((tab) => tab === value)
}

export const Route = createFileRoute('/_authed/settings/status-pages')({
  component: StatusPagesManagement,
  validateSearch: (search: Record<string, unknown>): { tab: StatusPageTab } => ({
    tab: isStatusPageTab(search.tab) ? search.tab : 'config'
  })
})

function StatusPagesManagement() {
  const { t } = useTranslation('settings')
  const { tab } = Route.useSearch()
  const navigate = Route.useNavigate()

  const { data: servers } = useQuery<ServerResponse[]>({
    queryKey: ['servers-list'],
    queryFn: () => api.get<ServerResponse[]>('/api/servers')
  })

  return (
    <div>
      <Tabs
        className="max-w-5xl"
        onValueChange={(value) => {
          if (isStatusPageTab(value)) {
            navigate({ search: { tab: value } })
          }
        }}
        value={tab}
      >
        <TabsList>
          <TabsTrigger value="config">{t('status_pages.tab_config')}</TabsTrigger>
          <TabsTrigger value="incidents">{t('status_pages.tab_incidents')}</TabsTrigger>
          <TabsTrigger value="maintenance">{t('status_pages.tab_maintenance')}</TabsTrigger>
        </TabsList>

        <TabsContent value="config">
          <StatusPageConfigForm servers={servers ?? []} />
        </TabsContent>

        <TabsContent value="incidents">
          <StatusPageIncidentsTab servers={servers ?? []} />
        </TabsContent>

        <TabsContent value="maintenance">
          <StatusPageMaintenanceTab servers={servers ?? []} />
        </TabsContent>
      </Tabs>
    </div>
  )
}
