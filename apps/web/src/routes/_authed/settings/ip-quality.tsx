import { createFileRoute } from '@tanstack/react-router'
import { Plus } from 'lucide-react'
import { type FormEvent, useCallback, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { CustomServiceDialog } from '@/components/ip-quality/custom-service-dialog'
import { Button } from '@/components/ui/button'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useAuth } from '@/hooks/use-auth'
import {
  useDeleteService,
  useIpQualityServices,
  useIpQualitySetting,
  useUpdateService,
  useUpdateSetting
} from '@/hooks/use-ip-quality-api'
import { categoryLabel } from '@/lib/ip-quality-constants'
import type { UnlockService } from '@/lib/ip-quality-types'
import { IpQualityCatalogTab } from './ip-quality-catalog-tab'
import { IpQualityDeleteDialog } from './ip-quality-delete-dialog'
import { IpQualitySettingsTab } from './ip-quality-settings-tab'

export const Route = createFileRoute('/_authed/settings/ip-quality')({
  validateSearch: (search: Record<string, unknown>) => ({
    tab: (search.tab as string) || 'catalog'
  }),
  component: IpQualitySettingsPage
})

export function IpQualitySettingsPage() {
  const { t } = useTranslation('ip-quality')
  const { tab: activeTab } = Route.useSearch()
  const navigate = Route.useNavigate()
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  // --- catalog tab state ---
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editingService, setEditingService] = useState<UnlockService | null>(null)
  const [deleteServiceId, setDeleteServiceId] = useState<string | null>(null)

  const { data: services = [], isLoading: servicesLoading } = useIpQualityServices()
  const { data: setting } = useIpQualitySetting()

  const updateService = useUpdateService()
  const deleteService = useDeleteService()
  const updateSetting = useUpdateSetting()
  const intervalHours = setting?.check_interval_hours ?? 12

  // Separate built-in services from custom ones, sort each group
  const builtinServices = useMemo(
    () =>
      services
        .filter((s) => s.is_builtin)
        .toSorted(
          (a, b) => categoryLabel(a.category).localeCompare(categoryLabel(b.category)) || b.popularity - a.popularity
        ),
    [services]
  )
  const customServices = useMemo(
    () => services.filter((s) => !s.is_builtin).toSorted((a, b) => a.name.localeCompare(b.name)),
    [services]
  )

  const handleToggleBuiltin = useCallback(
    (service: UnlockService) => {
      if (!isAdmin) {
        return
      }
      updateService.mutate(
        { id: service.id, enabled: !service.enabled },
        {
          onError: (err) => {
            toast.error(err instanceof Error ? err.message : t('settings_update_failed'))
          }
        }
      )
    },
    [isAdmin, updateService, t]
  )

  const openAddDialog = () => {
    setEditingService(null)
    setDialogOpen(true)
  }

  const openEditDialog = useCallback((service: UnlockService) => {
    setEditingService(service)
    setDialogOpen(true)
  }, [])

  const handleDeleteConfirm = () => {
    if (!deleteServiceId) {
      return
    }
    deleteService.mutate(deleteServiceId, {
      onSuccess: () => {
        toast.success(t('settings_deleted'))
        setDeleteServiceId(null)
      },
      onError: (err) => {
        toast.error(err instanceof Error ? err.message : t('settings_delete_failed'))
      }
    })
  }

  const handleSaveSettings = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const formData = new FormData(e.currentTarget)
    const checkIntervalHours = Number.parseInt(String(formData.get('check-interval') ?? ''), 10)

    updateSetting.mutate(
      { check_interval_hours: Number.isFinite(checkIntervalHours) ? checkIntervalHours : 12 },
      {
        onSuccess: () => {
          toast.success(t('settings_saved'))
        },
        onError: (err) => {
          toast.error(err instanceof Error ? err.message : t('settings_save_failed'))
        }
      }
    )
  }

  return (
    <div className="flex min-h-0 w-full min-w-0 max-w-[calc(100vw-1.5rem)] flex-1 flex-col overflow-hidden sm:max-w-full">
      <Tabs
        className="flex min-h-0 w-full min-w-0 max-w-full flex-1 flex-col"
        onValueChange={(value) => navigate({ search: { tab: value } })}
        value={activeTab}
      >
        <div className="flex w-full max-w-full flex-col items-stretch gap-3 sm:max-w-4xl sm:flex-row sm:items-center sm:justify-between">
          <TabsList className="w-full sm:w-auto">
            <TabsTrigger value="catalog">{t('settings_tab_catalog')}</TabsTrigger>
            <TabsTrigger value="settings">{t('settings_tab_settings')}</TabsTrigger>
          </TabsList>
          {activeTab === 'catalog' && isAdmin && (
            <Button className="w-full sm:w-auto" onClick={openAddDialog} size="sm" variant="outline">
              <Plus className="size-4" />
              {t('settings_add_custom')}
            </Button>
          )}
        </div>

        {/* Tab 1: Service Catalog */}
        <TabsContent className="flex min-h-0 flex-1 flex-col overflow-hidden" value="catalog">
          <IpQualityCatalogTab
            builtinServices={builtinServices}
            customServices={customServices}
            isAdmin={isAdmin}
            onDelete={setDeleteServiceId}
            onEdit={openEditDialog}
            onToggleBuiltin={handleToggleBuiltin}
            servicesLoading={servicesLoading}
            updateServicePending={updateService.isPending}
          />
        </TabsContent>

        {/* Tab 2: Settings */}
        <TabsContent className="min-h-0 overflow-hidden" value="settings">
          <IpQualitySettingsTab
            defaultIntervalHours={intervalHours}
            isAdmin={isAdmin}
            onSubmit={handleSaveSettings}
            updatePending={updateSetting.isPending}
          />
        </TabsContent>
      </Tabs>

      {/* Create / Edit custom service dialog */}
      <CustomServiceDialog
        onOpenChange={(open) => {
          setDialogOpen(open)
          if (!open) {
            setEditingService(null)
          }
        }}
        open={dialogOpen}
        service={editingService}
      />

      <IpQualityDeleteDialog
        onClose={() => setDeleteServiceId(null)}
        onConfirm={handleDeleteConfirm}
        open={deleteServiceId !== null}
        pending={deleteService.isPending}
      />
    </div>
  )
}
