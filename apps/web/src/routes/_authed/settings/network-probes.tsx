import { createFileRoute } from '@tanstack/react-router'
import { Plus } from 'lucide-react'
import { useCallback, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import {
  useCreateTarget,
  useDeleteTarget,
  useNetworkSetting,
  useNetworkTargets,
  useUpdateNetworkSetting,
  useUpdateTarget
} from '@/hooks/use-network-api'
import type { NetworkProbeSetting, NetworkProbeTarget } from '@/lib/network-types'
import { NetworkProbeDeleteDialog } from './network-probe-delete-dialog'
import { NetworkProbeSettingsTab } from './network-probe-settings-tab'
import { NetworkProbeTargetDialog, type TargetFormData } from './network-probe-target-dialog'
import { NetworkProbeTargetsTab } from './network-probe-targets-tab'

export const Route = createFileRoute('/_authed/settings/network-probes')({
  validateSearch: (search: Record<string, unknown>) => ({
    tab: (search.tab as string) || 'targets'
  }),
  component: NetworkProbeSettingsPage
})

function getCustomTargetCreatedAt(target: NetworkProbeTarget): number {
  if (target.source || !target.created_at) {
    return Number.NEGATIVE_INFINITY
  }

  const timestamp = Date.parse(target.created_at)
  return Number.isNaN(timestamp) ? Number.NEGATIVE_INFINITY : timestamp
}

function compareTargetsForDisplay(a: NetworkProbeTarget, b: NetworkProbeTarget): number {
  const aIsCustom = !a.source
  const bIsCustom = !b.source

  if (aIsCustom !== bIsCustom) {
    return aIsCustom ? -1 : 1
  }

  if (aIsCustom && bIsCustom) {
    return getCustomTargetCreatedAt(b) - getCustomTargetCreatedAt(a)
  }

  return 0
}

export function NetworkProbeSettingsPage() {
  const { t } = useTranslation('network')

  const { tab: activeTab } = Route.useSearch()
  const navigate = Route.useNavigate()

  // Target dialog state
  const [showDialog, setShowDialog] = useState(false)
  const [editingTarget, setEditingTarget] = useState<NetworkProbeTarget | null>(null)

  // Delete confirmation
  const [deleteTargetId, setDeleteTargetId] = useState<string | null>(null)

  const { data: targets, isLoading: targetsLoading } = useNetworkTargets()
  const { data: setting } = useNetworkSetting()

  const createTarget = useCreateTarget()
  const updateTarget = useUpdateTarget()
  const deleteTarget = useDeleteTarget()
  const updateSetting = useUpdateNetworkSetting()

  const openAddDialog = () => {
    setEditingTarget(null)
    setShowDialog(true)
  }

  const openEditDialog = useCallback((target: NetworkProbeTarget) => {
    setEditingTarget(target)
    setShowDialog(true)
  }, [])

  const closeDialog = () => {
    setShowDialog(false)
    setEditingTarget(null)
  }

  const handleSubmitTarget = (form: TargetFormData) => {
    if (!(form.name.trim() && form.target.trim())) {
      return
    }

    if (editingTarget) {
      updateTarget.mutate(
        { id: editingTarget.id, ...form },
        {
          onSuccess: () => {
            toast.success(t('target_updated', { defaultValue: 'Target updated' }))
            closeDialog()
          },
          onError: (err) => {
            toast.error(
              err instanceof Error
                ? err.message
                : t('target_update_failed', { defaultValue: 'Failed to update target' })
            )
          }
        }
      )
    } else {
      createTarget.mutate(form, {
        onSuccess: () => {
          toast.success(t('target_created', { defaultValue: 'Target created' }))
          closeDialog()
        },
        onError: (err) => {
          toast.error(
            err instanceof Error ? err.message : t('target_create_failed', { defaultValue: 'Failed to create target' })
          )
        }
      })
    }
  }

  const handleDeleteConfirm = () => {
    if (!deleteTargetId) {
      return
    }
    deleteTarget.mutate(deleteTargetId, {
      onSuccess: () => {
        toast.success(t('target_deleted', { defaultValue: 'Target deleted' }))
        setDeleteTargetId(null)
      },
      onError: (err) => {
        toast.error(
          err instanceof Error ? err.message : t('target_delete_failed', { defaultValue: 'Failed to delete target' })
        )
      }
    })
  }

  const handleSaveSettings = (nextSetting: NetworkProbeSetting) => {
    updateSetting.mutate(nextSetting, {
      onSuccess: () => {
        toast.success(t('settings_saved', { defaultValue: 'Settings saved' }))
      },
      onError: (err) => {
        toast.error(
          err instanceof Error ? err.message : t('settings_save_failed', { defaultValue: 'Failed to save settings' })
        )
      }
    })
  }

  const sortedTargets = useMemo(() => (targets ?? []).toSorted(compareTargetsForDisplay), [targets])
  const settingVersion = setting
    ? `${setting.interval}:${setting.packet_count}:${setting.default_target_ids.join(',')}`
    : 'default-setting'

  return (
    <div className="flex min-h-0 w-full min-w-0 max-w-[calc(100vw-1.5rem)] flex-1 flex-col overflow-hidden sm:max-w-full">
      <Tabs
        className="flex min-h-0 w-full min-w-0 max-w-full flex-1 flex-col"
        onValueChange={(value) => navigate({ search: { tab: value } })}
        value={activeTab}
      >
        <div className="flex w-full max-w-full flex-col items-stretch gap-3 sm:max-w-4xl sm:flex-row sm:items-center sm:justify-between">
          <TabsList className="w-full sm:w-auto">
            <TabsTrigger value="targets">{t('target_management')}</TabsTrigger>
            <TabsTrigger value="settings">{t('global_settings')}</TabsTrigger>
          </TabsList>
          {activeTab === 'targets' && (
            <Button className="w-full sm:w-auto" onClick={openAddDialog} size="sm" variant="outline">
              <Plus className="size-4" />
              {t('add_target')}
            </Button>
          )}
        </div>

        {/* Tab 1: Target Management */}
        <TabsContent className="flex min-h-0 flex-1 flex-col overflow-hidden" value="targets">
          <NetworkProbeTargetsTab
            onDelete={setDeleteTargetId}
            onEdit={openEditDialog}
            targets={sortedTargets}
            targetsLoading={targetsLoading}
          />
        </TabsContent>

        {/* Tab 2: Global Settings */}
        <TabsContent className="min-h-0 overflow-hidden" value="settings">
          <NetworkProbeSettingsTab
            key={settingVersion}
            onSubmit={handleSaveSettings}
            setting={setting}
            targets={sortedTargets}
            updatePending={updateSetting.isPending}
          />
        </TabsContent>
      </Tabs>

      <NetworkProbeTargetDialog
        createPending={createTarget.isPending}
        key={editingTarget ? `${editingTarget.id}:${editingTarget.updated_at ?? ''}` : 'new-target'}
        onClose={closeDialog}
        onSubmit={handleSubmitTarget}
        open={showDialog}
        target={editingTarget}
        updatePending={updateTarget.isPending}
      />

      <NetworkProbeDeleteDialog
        onClose={() => setDeleteTargetId(null)}
        onConfirm={handleDeleteConfirm}
        open={deleteTargetId !== null}
        pending={deleteTarget.isPending}
      />
    </div>
  )
}
