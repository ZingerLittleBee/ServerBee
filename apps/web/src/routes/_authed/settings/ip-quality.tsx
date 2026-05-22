import { createFileRoute } from '@tanstack/react-router'
import { type ColumnDef, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import { MoreHorizontal, Pencil, Plus, Trash2 } from 'lucide-react'
import { type FormEvent, useCallback, useEffect, useMemo, useState } from 'react'
import { toast } from 'sonner'
import { CustomServiceDialog } from '@/components/ip-quality/custom-service-dialog'
import { UnlockStatusBadge } from '@/components/ip-quality/unlock-status-badge'
import { Button } from '@/components/ui/button'
import { DataTable } from '@/components/ui/data-table'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useAuth } from '@/hooks/use-auth'
import {
  useDeleteService,
  useIpQualityServices,
  useIpQualitySetting,
  useUpdateService,
  useUpdateSetting
} from '@/hooks/use-ip-quality-api'
import type { UnlockService } from '@/lib/ip-quality-types'

export const Route = createFileRoute('/_authed/settings/ip-quality')({
  validateSearch: (search: Record<string, unknown>) => ({
    tab: (search.tab as string) || 'catalog'
  }),
  component: IpQualitySettingsPage
})

const CATEGORY_LABELS: Record<string, string> = {
  streaming: 'Streaming',
  ai: 'AI',
  social: 'Social',
  gaming: 'Gaming',
  other: 'Other'
}

function categoryLabel(cat: string): string {
  return CATEGORY_LABELS[cat] ?? cat
}

export function IpQualitySettingsPage() {
  const { tab: activeTab } = Route.useSearch()
  const navigate = Route.useNavigate()
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  // --- catalog tab state ---
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editingService, setEditingService] = useState<UnlockService | null>(null)
  const [deleteServiceId, setDeleteServiceId] = useState<string | null>(null)

  // --- settings tab state ---
  const [intervalHours, setIntervalHours] = useState(12)

  const { data: services = [], isLoading: servicesLoading } = useIpQualityServices()
  const { data: setting } = useIpQualitySetting()

  useEffect(() => {
    if (setting) {
      setIntervalHours(setting.check_interval_hours)
    }
  }, [setting])

  const updateService = useUpdateService()
  const deleteService = useDeleteService()
  const updateSetting = useUpdateSetting()

  // Separate built-in services from custom ones, sort each group
  const builtinServices = useMemo(
    () =>
      [...services.filter((s) => s.is_builtin)].sort(
        (a, b) => categoryLabel(a.category).localeCompare(categoryLabel(b.category)) || b.popularity - a.popularity
      ),
    [services]
  )
  const customServices = useMemo(
    () => [...services.filter((s) => !s.is_builtin)].sort((a, b) => a.name.localeCompare(b.name)),
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
            toast.error(err instanceof Error ? err.message : 'Failed to update service')
          }
        }
      )
    },
    [isAdmin, updateService]
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
        toast.success('Service deleted')
        setDeleteServiceId(null)
      },
      onError: (err) => {
        toast.error(err instanceof Error ? err.message : 'Failed to delete service')
      }
    })
  }

  const handleSaveSettings = (e: FormEvent) => {
    e.preventDefault()
    updateSetting.mutate(
      { check_interval_hours: intervalHours },
      {
        onSuccess: () => {
          toast.success('Settings saved')
        },
        onError: (err) => {
          toast.error(err instanceof Error ? err.message : 'Failed to save settings')
        }
      }
    )
  }

  const customColumns = useMemo<ColumnDef<UnlockService>[]>(
    () => [
      {
        accessorKey: 'name',
        header: 'Name',
        enableSorting: false,
        cell: ({ row }) => <span className="font-medium">{row.original.name}</span>
      },
      {
        accessorKey: 'category',
        header: 'Category',
        enableSorting: false,
        cell: ({ row }) => <span className="text-muted-foreground">{categoryLabel(row.original.category)}</span>
      },
      {
        accessorKey: 'enabled',
        header: 'Status',
        enableSorting: false,
        cell: ({ row }) => <UnlockStatusBadge status={row.original.enabled ? 'unlocked' : 'blocked'} />
      },
      {
        id: 'actions',
        header: 'Manage',
        enableSorting: false,
        meta: { className: 'text-right' },
        cell: ({ row }) => (
          <div className="flex justify-end">
            <DropdownMenu>
              <DropdownMenuTrigger
                aria-label={`More actions for ${row.original.name}`}
                render={<Button className="ml-auto" size="icon-sm" variant="ghost" />}
              >
                <MoreHorizontal aria-hidden="true" className="size-4" />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-36">
                <DropdownMenuItem
                  aria-label={`Edit ${row.original.name}`}
                  disabled={!isAdmin}
                  onClick={() => openEditDialog(row.original)}
                >
                  <Pencil className="size-3.5" />
                  Edit
                </DropdownMenuItem>
                <DropdownMenuItem
                  aria-label={`Delete ${row.original.name}`}
                  disabled={!isAdmin}
                  onClick={() => setDeleteServiceId(row.original.id)}
                  variant="destructive"
                >
                  <Trash2 className="size-3.5" />
                  Delete
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        )
      }
    ],
    [isAdmin, openEditDialog]
  )

  const customTable = useReactTable({
    data: customServices,
    columns: customColumns,
    getCoreRowModel: getCoreRowModel(),
    getRowId: (row) => row.id
  })

  return (
    <div className="flex min-h-0 w-full min-w-0 max-w-[calc(100vw-1.5rem)] flex-1 flex-col overflow-hidden sm:max-w-full">
      <h1 className="mb-6 min-w-0 font-bold text-2xl">IP Quality</h1>
      <Tabs
        className="flex min-h-0 w-full min-w-0 max-w-full flex-1 flex-col"
        onValueChange={(value) => navigate({ search: { tab: value } })}
        value={activeTab}
      >
        <div className="flex w-full max-w-full flex-col items-stretch gap-3 sm:max-w-4xl sm:flex-row sm:items-center sm:justify-between">
          <TabsList className="w-full sm:w-auto">
            <TabsTrigger value="catalog">Service Catalog</TabsTrigger>
            <TabsTrigger value="settings">Settings</TabsTrigger>
          </TabsList>
          {activeTab === 'catalog' && isAdmin && (
            <Button className="w-full sm:w-auto" onClick={openAddDialog} size="sm" variant="outline">
              <Plus className="size-4" />
              Add custom service
            </Button>
          )}
        </div>

        {/* Tab 1: Service Catalog */}
        <TabsContent className="flex min-h-0 flex-1 flex-col overflow-hidden" value="catalog">
          {servicesLoading && (
            <div className="max-w-4xl space-y-2 p-4">
              {Array.from({ length: 4 }, (_, i) => (
                <Skeleton className="h-10" key={`skel-${i.toString()}`} />
              ))}
            </div>
          )}

          {!servicesLoading && (
            <ScrollArea className="h-full">
              <div className="max-w-4xl space-y-6 pb-4">
                {/* Built-in services */}
                <div className="space-y-2">
                  <h2 className="font-semibold text-muted-foreground text-sm uppercase tracking-wide">
                    Built-in services
                  </h2>
                  <div className="rounded-md border">
                    {builtinServices.length === 0 && (
                      <p className="px-4 py-3 text-muted-foreground text-sm">No built-in services found.</p>
                    )}
                    {builtinServices.map((service, idx) => (
                      <div
                        className={`flex items-center justify-between px-4 py-2.5 ${idx < builtinServices.length - 1 ? 'border-b' : ''}`}
                        key={service.id}
                      >
                        <div className="flex min-w-0 flex-col">
                          <span className="font-medium text-sm">{service.name}</span>
                          <span className="text-muted-foreground text-xs">{categoryLabel(service.category)}</span>
                        </div>
                        <Switch
                          aria-label={`${service.enabled ? 'Disable' : 'Enable'} ${service.name}`}
                          checked={service.enabled}
                          disabled={!isAdmin || updateService.isPending}
                          onCheckedChange={() => handleToggleBuiltin(service)}
                        />
                      </div>
                    ))}
                  </div>
                </div>

                {/* Custom services */}
                <div className="space-y-2">
                  <h2 className="font-semibold text-muted-foreground text-sm uppercase tracking-wide">
                    Custom services
                  </h2>
                  <DataTable
                    className="w-full min-w-0 max-w-full"
                    noResults="No custom services yet."
                    table={customTable}
                  />
                </div>
              </div>
            </ScrollArea>
          )}
        </TabsContent>

        {/* Tab 2: Settings */}
        <TabsContent className="min-h-0 overflow-hidden" value="settings">
          <ScrollArea className="h-full">
            <form className="max-w-xl space-y-6 pb-1" onSubmit={handleSaveSettings}>
              <div className="space-y-1.5">
                <label className="font-medium text-sm" htmlFor="check-interval">
                  Check interval (hours)
                </label>
                <Input
                  autoComplete="off"
                  disabled={!isAdmin}
                  id="check-interval"
                  max={168}
                  min={1}
                  name="check-interval"
                  onChange={(e) => setIntervalHours(Number.parseInt(e.target.value, 10) || 12)}
                  type="number"
                  value={intervalHours}
                />
                <p className="text-muted-foreground text-xs">
                  How often each capable agent checks its service unlock status and egress IP. Default: 12 hours.
                </p>
              </div>

              <Button disabled={!isAdmin || updateSetting.isPending} size="sm" type="submit">
                Save
              </Button>
            </form>
          </ScrollArea>
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

      {/* Delete confirmation dialog */}
      <Dialog
        onOpenChange={(open) => {
          if (!open) {
            setDeleteServiceId(null)
          }
        }}
        open={deleteServiceId !== null}
      >
        <DialogContent className="sm:max-w-sm" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>Delete custom service</DialogTitle>
          </DialogHeader>
          <p className="text-muted-foreground text-sm">
            This action cannot be undone. The service and all its stored results will be deleted.
          </p>
          <div className="flex gap-2">
            <Button disabled={deleteService.isPending} onClick={handleDeleteConfirm} size="sm" variant="destructive">
              <Trash2 className="mr-1 size-3.5" />
              Delete
            </Button>
            <Button onClick={() => setDeleteServiceId(null)} size="sm" type="button" variant="ghost">
              Cancel
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}
