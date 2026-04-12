import { createFileRoute } from '@tanstack/react-router'
import { type ColumnDef, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import { Lock, Pencil, Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { DataTable } from '@/components/ui/data-table'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
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
import { getNetworkProbeTypeLabel, getNetworkTargetDisplayName } from '@/lib/network-i18n'
import type { NetworkProbeTarget } from '@/lib/network-types'

export const Route = createFileRoute('/_authed/settings/network-probes')({
  validateSearch: (search: Record<string, unknown>) => ({
    tab: (search.tab as string) || 'targets'
  }),
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

export function NetworkProbeSettingsPage() {
  const { t, i18n } = useTranslation('network')

  const { tab: activeTab } = Route.useSearch()
  const navigate = Route.useNavigate()

  // Target dialog state
  const [showDialog, setShowDialog] = useState(false)
  const [editingTarget, setEditingTarget] = useState<NetworkProbeTarget | null>(null)
  const [form, setForm] = useState<TargetFormData>(DEFAULT_FORM)

  // Delete confirmation
  const [deleteTargetId, setDeleteTargetId] = useState<string | null>(null)

  // Settings form state
  const [probeInterval, setProbeInterval] = useState(60)
  const [packetCount, setPacketCount] = useState(10)
  const [defaultTargetIds, setDefaultTargetIds] = useState<string[]>([])

  const { data: targets, isLoading: targetsLoading } = useNetworkTargets()
  const { data: setting } = useNetworkSetting()

  // Sync settings into local state once loaded
  useEffect(() => {
    if (setting) {
      setProbeInterval(setting.interval)
      setPacketCount(setting.packet_count)
      setDefaultTargetIds(setting.default_target_ids)
    }
  }, [setting])

  const createTarget = useCreateTarget()
  const updateTarget = useUpdateTarget()
  const deleteTarget = useDeleteTarget()
  const updateSetting = useUpdateNetworkSetting()

  const openAddDialog = () => {
    setEditingTarget(null)
    setForm(DEFAULT_FORM)
    setShowDialog(true)
  }

  const openEditDialog = useCallback((target: NetworkProbeTarget) => {
    setEditingTarget(target)
    setForm({
      name: target.name,
      provider: target.provider,
      location: target.location,
      target: target.target,
      probe_type: target.probe_type as ProbeType
    })
    setShowDialog(true)
  }, [])

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
      { interval: probeInterval, packet_count: packetCount, default_target_ids: defaultTargetIds },
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

  const getProbeTypeLabel = useCallback((probeType: string) => getNetworkProbeTypeLabel(t, probeType), [t])
  const getTargetDisplayName = useCallback(
    (target: NetworkProbeTarget) => getNetworkTargetDisplayName(t, i18n.resolvedLanguage ?? i18n.language, target),
    [t, i18n.language, i18n.resolvedLanguage]
  )

  const probeTypes: { value: ProbeType; label: string }[] = [
    { value: 'icmp', label: getProbeTypeLabel('icmp') },
    { value: 'tcp', label: getProbeTypeLabel('tcp') },
    { value: 'http', label: getProbeTypeLabel('http') }
  ]

  const targetColumns = useMemo<ColumnDef<NetworkProbeTarget>[]>(
    () => [
      {
        accessorKey: 'name',
        header: () => t('target_name'),
        enableSorting: false,
        cell: ({ row }) => <span className="font-medium">{getTargetDisplayName(row.original)}</span>
      },
      {
        accessorKey: 'provider',
        header: () => t('target_provider'),
        enableSorting: false,
        cell: ({ row }) => <span className="text-muted-foreground">{row.original.provider || '\u2014'}</span>
      },
      {
        accessorKey: 'location',
        header: () => t('target_location'),
        enableSorting: false,
        cell: ({ row }) => <span className="text-muted-foreground">{row.original.location || '\u2014'}</span>
      },
      {
        accessorKey: 'target',
        header: () => t('target_address'),
        enableSorting: false,
        cell: ({ row }) => <span className="font-mono text-muted-foreground text-xs">{row.original.target}</span>
      },
      {
        accessorKey: 'probe_type',
        header: () => t('target_type'),
        enableSorting: false,
        cell: ({ row }) => (
          <span className="rounded-full bg-muted px-2 py-0.5 text-xs">
            {getProbeTypeLabel(row.original.probe_type)}
          </span>
        )
      },
      {
        accessorKey: 'source',
        header: () => t('target_status', { defaultValue: 'Status' }),
        enableSorting: false,
        cell: ({ row }) =>
          row.original.source ? (
            <span className="flex items-center gap-1 text-muted-foreground text-xs">
              <Lock aria-hidden="true" className="size-3" />
              {row.original.source_name ?? t('builtin', { defaultValue: 'Built-in' })}
            </span>
          ) : (
            <span className="text-muted-foreground text-xs">{t('custom')}</span>
          )
      },
      {
        id: 'actions',
        header: () => t('target_actions', { defaultValue: 'Actions' }),
        enableSorting: false,
        meta: { className: 'text-right' },
        cell: ({ row }) =>
          !row.original.source && (
            <div className="flex justify-end gap-1">
              <Button
                aria-label={t('edit_target_aria', { defaultValue: 'Edit {{name}}', name: row.original.name })}
                onClick={() => openEditDialog(row.original)}
                size="sm"
                variant="outline"
              >
                <Pencil className="size-3.5" />
              </Button>
              <Button
                aria-label={t('delete_target_aria', { defaultValue: 'Delete {{name}}', name: row.original.name })}
                onClick={() => setDeleteTargetId(row.original.id)}
                size="sm"
                variant="destructive"
              >
                <Trash2 className="size-3.5" />
              </Button>
            </div>
          )
      }
    ],
    [t, openEditDialog, getProbeTypeLabel, getTargetDisplayName]
  )

  const targetsTable = useReactTable({
    data: targets ?? [],
    columns: targetColumns,
    getCoreRowModel: getCoreRowModel(),
    getRowId: (row) => row.id
  })

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <h1 className="mb-6 font-bold text-2xl">{t('settings_title')}</h1>
      <Tabs
        className="flex min-h-0 flex-1 flex-col"
        onValueChange={(value) => navigate({ search: { tab: value } })}
        value={activeTab}
      >
        <div className="flex max-w-4xl items-center justify-between">
          <TabsList>
            <TabsTrigger value="targets">{t('target_management')}</TabsTrigger>
            <TabsTrigger value="settings">{t('global_settings')}</TabsTrigger>
          </TabsList>
          {activeTab === 'targets' && (
            <Button onClick={openAddDialog} size="sm" variant="outline">
              <Plus className="size-4" />
              {t('add_target')}
            </Button>
          )}
        </div>

        {/* Tab 1: Target Management */}
        <TabsContent className="flex min-h-0 flex-1 flex-col overflow-hidden" value="targets">
          {targetsLoading && (
            <div className="max-w-4xl space-y-2 p-4">
              {Array.from({ length: 3 }, (_, i) => (
                <Skeleton className="h-10" key={`skel-${i.toString()}`} />
              ))}
            </div>
          )}

          {!targetsLoading && (
            <DataTable className="flex h-full max-w-4xl flex-col" noResults={t('no_targets')} table={targetsTable} />
          )}
        </TabsContent>

        {/* Tab 2: Global Settings */}
        <TabsContent className="min-h-0 overflow-hidden" value="settings">
          <ScrollArea className="h-full">
            <form className="max-w-xl space-y-6 pb-1" onSubmit={handleSaveSettings}>
              <div className="space-y-1.5">
                <label className="font-medium text-sm" htmlFor="probe-interval">
                  {t('probe_interval')}
                </label>
                <Input
                  autoComplete="off"
                  id="probe-interval"
                  max={600}
                  min={30}
                  name="probe-interval"
                  onChange={(e) => setProbeInterval(Number.parseInt(e.target.value, 10) || 60)}
                  type="number"
                  value={probeInterval}
                />
                <p className="text-muted-foreground text-xs">{t('probe_interval_desc')}</p>
              </div>

              <div className="space-y-1.5">
                <label className="font-medium text-sm" htmlFor="packet-count">
                  {t('packet_count')}
                </label>
                <Input
                  autoComplete="off"
                  id="packet-count"
                  max={20}
                  min={5}
                  name="packet-count"
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
                  <ScrollArea className="h-72 rounded-md border p-3">
                    <div className="space-y-1.5">
                      {targets.map((target) => (
                        // biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element
                        <label className="flex cursor-pointer items-center gap-2 text-sm" key={target.id}>
                          <Checkbox
                            checked={defaultTargetIds.includes(target.id)}
                            onCheckedChange={() => toggleDefaultTarget(target.id)}
                          />
                          <span>{getTargetDisplayName(target)}</span>
                          <span className="text-muted-foreground text-xs">
                            ({getProbeTypeLabel(target.probe_type)})
                          </span>
                        </label>
                      ))}
                    </div>
                  </ScrollArea>
                ) : (
                  <p className="text-muted-foreground text-xs">{t('no_targets')}</p>
                )}
              </div>

              <Button disabled={updateSetting.isPending} size="sm" type="submit">
                {t('save')}
              </Button>
            </form>
          </ScrollArea>
        </TabsContent>
      </Tabs>
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
                autoComplete="off"
                id="form-name"
                name="target-name"
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
                autoComplete="off"
                id="form-provider"
                name="target-provider"
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
                autoComplete="off"
                id="form-location"
                name="target-location"
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
                autoComplete="off"
                id="form-target"
                name="target-address"
                onChange={(e) => setForm((prev) => ({ ...prev, target: e.target.value }))}
                placeholder={t('target_address_placeholder', { defaultValue: 'e.g. 1.1.1.1 or example.com:80' })}
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
                items={probeTypes}
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
