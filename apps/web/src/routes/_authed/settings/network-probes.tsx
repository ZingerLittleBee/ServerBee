import { createFileRoute } from '@tanstack/react-router'
import { Lock, Pencil, Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import {
  useCreateTarget,
  useDeleteTarget,
  useNetworkSetting,
  useNetworkTargets,
  useUpdateNetworkSetting,
  useUpdateTarget
} from '@/hooks/use-network-api'
import type { NetworkProbeTarget } from '@/lib/network-types'

export const Route = createFileRoute('/_authed/settings/network-probes')({
  component: NetworkProbeSettingsPage
})

type ProbeType = 'icmp' | 'tcp' | 'http'

interface TargetFormData {
  location: string
  name: string
  probe_type: ProbeType
  provider: string
  target: string
}

const DEFAULT_FORM: TargetFormData = {
  name: '',
  provider: '',
  location: '',
  target: '',
  probe_type: 'icmp'
}

function NetworkProbeSettingsPage() {
  const { t } = useTranslation('network')

  const [activeTab, setActiveTab] = useState<string>('targets')

  // Target dialog state
  const [showDialog, setShowDialog] = useState(false)
  const [editingTarget, setEditingTarget] = useState<NetworkProbeTarget | null>(null)
  const [form, setForm] = useState<TargetFormData>(DEFAULT_FORM)

  // Delete confirmation
  const [deleteTargetId, setDeleteTargetId] = useState<string | null>(null)

  // Settings form state
  const [interval, setInterval] = useState(60)
  const [packetCount, setPacketCount] = useState(10)
  const [defaultTargetIds, setDefaultTargetIds] = useState<string[]>([])
  const [settingsLoaded, setSettingsLoaded] = useState(false)

  const { data: targets, isLoading: targetsLoading } = useNetworkTargets()
  const { data: setting } = useNetworkSetting()

  // Sync settings into local state once loaded
  if (setting && !settingsLoaded) {
    setInterval(setting.interval)
    setPacketCount(setting.packet_count)
    setDefaultTargetIds(setting.default_target_ids)
    setSettingsLoaded(true)
  }

  const createTarget = useCreateTarget()
  const updateTarget = useUpdateTarget()
  const deleteTarget = useDeleteTarget()
  const updateSetting = useUpdateNetworkSetting()

  const openAddDialog = () => {
    setEditingTarget(null)
    setForm(DEFAULT_FORM)
    setShowDialog(true)
  }

  const openEditDialog = (target: NetworkProbeTarget) => {
    setEditingTarget(target)
    setForm({
      name: target.name,
      provider: target.provider,
      location: target.location,
      target: target.target,
      probe_type: target.probe_type as ProbeType
    })
    setShowDialog(true)
  }

  const closeDialog = () => {
    setShowDialog(false)
    setEditingTarget(null)
    setForm(DEFAULT_FORM)
  }

  const handleSubmitTarget = (e: FormEvent) => {
    e.preventDefault()
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

  const handleSaveSettings = (e: FormEvent) => {
    e.preventDefault()
    updateSetting.mutate(
      { interval, packet_count: packetCount, default_target_ids: defaultTargetIds },
      {
        onSuccess: () => {
          toast.success(t('settings_saved', { defaultValue: 'Settings saved' }))
        },
        onError: (err) => {
          toast.error(
            err instanceof Error ? err.message : t('settings_save_failed', { defaultValue: 'Failed to save settings' })
          )
        }
      }
    )
  }

  const toggleDefaultTarget = (id: string) => {
    setDefaultTargetIds((prev) => (prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]))
  }

  const probeTypes: { value: ProbeType; label: string }[] = [
    { value: 'icmp', label: 'ICMP (Ping)' },
    { value: 'tcp', label: 'TCP' },
    { value: 'http', label: 'HTTP' }
  ]

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">{t('settings_title')}</h1>

      <Tabs onValueChange={setActiveTab} value={activeTab}>
        <TabsList>
          <TabsTrigger value="targets">{t('target_management')}</TabsTrigger>
          <TabsTrigger value="settings">{t('global_settings')}</TabsTrigger>
        </TabsList>

        {/* Tab 1: Target Management */}
        <TabsContent value="targets">
          <div className="max-w-4xl">
            <div className="rounded-lg border bg-card p-6">
              <div className="mb-4 flex items-center justify-between">
                <h2 className="font-semibold text-lg">{t('target_management')}</h2>
                <Button onClick={openAddDialog} size="sm" variant="outline">
                  <Plus className="size-4" />
                  {t('add_target')}
                </Button>
              </div>

              {targetsLoading && (
                <div className="space-y-2">
                  {Array.from({ length: 3 }, (_, i) => (
                    <Skeleton className="h-10" key={`skel-${i.toString()}`} />
                  ))}
                </div>
              )}

              {!targetsLoading && (!targets || targets.length === 0) && (
                <p className="py-6 text-center text-muted-foreground text-sm">{t('no_targets')}</p>
              )}

              {targets && targets.length > 0 && (
                <div className="overflow-x-auto">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="border-b text-muted-foreground text-xs">
                        <th className="pb-2 text-left font-medium">{t('target_name')}</th>
                        <th className="pb-2 text-left font-medium">{t('target_provider')}</th>
                        <th className="pb-2 text-left font-medium">{t('target_location')}</th>
                        <th className="pb-2 text-left font-medium">{t('target_address')}</th>
                        <th className="pb-2 text-left font-medium">{t('target_type')}</th>
                        <th className="pb-2 text-left font-medium">Status</th>
                        <th className="pb-2 text-right font-medium">Actions</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y">
                      {targets.map((target) => (
                        <tr className="hover:bg-muted/30" key={target.id}>
                          <td className="py-2.5 pr-4 font-medium">{target.name}</td>
                          <td className="py-2.5 pr-4 text-muted-foreground">{target.provider || '—'}</td>
                          <td className="py-2.5 pr-4 text-muted-foreground">{target.location || '—'}</td>
                          <td className="py-2.5 pr-4 font-mono text-muted-foreground text-xs">{target.target}</td>
                          <td className="py-2.5 pr-4">
                            <span className="rounded-full bg-muted px-2 py-0.5 text-xs uppercase">
                              {target.probe_type}
                            </span>
                          </td>
                          <td className="py-2.5 pr-4">
                            {target.is_builtin ? (
                              <span className="flex items-center gap-1 text-muted-foreground text-xs">
                                <Lock className="size-3" />
                                {t('builtin')}
                              </span>
                            ) : (
                              <span className="text-green-600 text-xs dark:text-green-400">{t('custom')}</span>
                            )}
                          </td>
                          <td className="py-2.5 text-right">
                            {!target.is_builtin && (
                              <div className="flex justify-end gap-1">
                                <Button
                                  aria-label={`Edit ${target.name}`}
                                  onClick={() => openEditDialog(target)}
                                  size="sm"
                                  variant="outline"
                                >
                                  <Pencil className="size-3.5" />
                                </Button>
                                <Button
                                  aria-label={`Delete ${target.name}`}
                                  onClick={() => setDeleteTargetId(target.id)}
                                  size="sm"
                                  variant="destructive"
                                >
                                  <Trash2 className="size-3.5" />
                                </Button>
                              </div>
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          </div>
        </TabsContent>

        {/* Tab 2: Global Settings */}
        <TabsContent value="settings">
          <div className="max-w-xl">
            <form className="space-y-6 rounded-lg border bg-card p-6" onSubmit={handleSaveSettings}>
              <h2 className="font-semibold text-lg">{t('global_settings')}</h2>

              <div className="space-y-1.5">
                <label className="font-medium text-sm" htmlFor="probe-interval">
                  {t('probe_interval')}
                </label>
                <Input
                  id="probe-interval"
                  max={600}
                  min={30}
                  onChange={(e) => setInterval(Number.parseInt(e.target.value, 10) || 60)}
                  type="number"
                  value={interval}
                />
                <p className="text-muted-foreground text-xs">{t('probe_interval_desc')}</p>
              </div>

              <div className="space-y-1.5">
                <label className="font-medium text-sm" htmlFor="packet-count">
                  {t('packet_count')}
                </label>
                <Input
                  id="packet-count"
                  max={20}
                  min={5}
                  onChange={(e) => setPacketCount(Number.parseInt(e.target.value, 10) || 10)}
                  type="number"
                  value={packetCount}
                />
                <p className="text-muted-foreground text-xs">{t('packet_count_desc')}</p>
              </div>

              <div className="space-y-2">
                <p className="font-medium text-sm">{t('default_targets')}</p>
                <p className="text-muted-foreground text-xs">{t('default_targets_desc')}</p>
                {targets && targets.length > 0 ? (
                  <div className="max-h-48 space-y-1.5 overflow-y-auto rounded-md border p-3">
                    {targets.map((target) => (
                      <label className="flex cursor-pointer items-center gap-2 text-sm" key={target.id}>
                        <input
                          checked={defaultTargetIds.includes(target.id)}
                          onChange={() => toggleDefaultTarget(target.id)}
                          type="checkbox"
                        />
                        <span>{target.name}</span>
                        <span className="text-muted-foreground text-xs">({target.probe_type.toUpperCase()})</span>
                      </label>
                    ))}
                  </div>
                ) : (
                  <p className="text-muted-foreground text-xs">{t('no_targets')}</p>
                )}
              </div>

              <Button disabled={updateSetting.isPending} size="sm" type="submit">
                {t('save')}
              </Button>
            </form>
          </div>
        </TabsContent>
      </Tabs>

      {/* Add/Edit Target Dialog */}
      <Dialog
        onOpenChange={(open) => {
          if (!open) {
            closeDialog()
          }
        }}
        open={showDialog}
      >
        <DialogContent className="sm:max-w-md" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>{editingTarget ? t('edit_target') : t('add_target')}</DialogTitle>
          </DialogHeader>
          <form className="space-y-3" onSubmit={handleSubmitTarget}>
            <div className="space-y-1">
              <label className="font-medium text-sm" htmlFor="form-name">
                {t('target_name')}
              </label>
              <Input
                id="form-name"
                onChange={(e) => setForm((prev) => ({ ...prev, name: e.target.value }))}
                placeholder={t('target_name')}
                required
                type="text"
                value={form.name}
              />
            </div>
            <div className="space-y-1">
              <label className="font-medium text-sm" htmlFor="form-provider">
                {t('target_provider')}
              </label>
              <Input
                id="form-provider"
                onChange={(e) => setForm((prev) => ({ ...prev, provider: e.target.value }))}
                placeholder={t('target_provider')}
                type="text"
                value={form.provider}
              />
            </div>
            <div className="space-y-1">
              <label className="font-medium text-sm" htmlFor="form-location">
                {t('target_location')}
              </label>
              <Input
                id="form-location"
                onChange={(e) => setForm((prev) => ({ ...prev, location: e.target.value }))}
                placeholder={t('target_location')}
                type="text"
                value={form.location}
              />
            </div>
            <div className="space-y-1">
              <label className="font-medium text-sm" htmlFor="form-target">
                {t('target_address')}
              </label>
              <Input
                id="form-target"
                onChange={(e) => setForm((prev) => ({ ...prev, target: e.target.value }))}
                placeholder="e.g. 1.1.1.1 or example.com:80"
                required
                type="text"
                value={form.target}
              />
            </div>
            <div className="space-y-1">
              <label className="font-medium text-sm" htmlFor="form-probe-type">
                {t('target_type')}
              </label>
              <Select
                onValueChange={(value) => setForm((prev) => ({ ...prev, probe_type: value as ProbeType }))}
                value={form.probe_type}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {probeTypes.map((pt) => (
                    <SelectItem key={pt.value} value={pt.value}>
                      {pt.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex gap-2 pt-2">
              <Button disabled={createTarget.isPending || updateTarget.isPending} size="sm" type="submit">
                {editingTarget ? t('save') : t('add_target')}
              </Button>
              <Button onClick={closeDialog} size="sm" type="button" variant="ghost">
                {t('cancel')}
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* Delete Confirmation Dialog */}
      <Dialog
        onOpenChange={(open) => {
          if (!open) {
            setDeleteTargetId(null)
          }
        }}
        open={deleteTargetId !== null}
      >
        <DialogContent className="sm:max-w-sm" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>{t('delete_target')}</DialogTitle>
          </DialogHeader>
          <p className="text-muted-foreground text-sm">{t('confirm_delete_target')}</p>
          <div className="flex gap-2">
            <Button disabled={deleteTarget.isPending} onClick={handleDeleteConfirm} size="sm" variant="destructive">
              <Trash2 className="mr-1 size-3.5" />
              {t('delete_target')}
            </Button>
            <Button onClick={() => setDeleteTargetId(null)} size="sm" type="button" variant="ghost">
              {t('cancel')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}
