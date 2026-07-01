import { type ColumnDef, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import { MoreHorizontal, Pencil, Trash2 } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { UnlockStatusBadge } from '@/components/ip-quality/unlock-status-badge'
import { Button } from '@/components/ui/button'
import { DataTable } from '@/components/ui/data-table'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { categoryLabel } from '@/lib/ip-quality-constants'
import type { UnlockService } from '@/lib/ip-quality-types'

export function IpQualityCatalogTab({
  builtinServices,
  customServices,
  isAdmin,
  onDelete,
  onEdit,
  onToggleBuiltin,
  servicesLoading,
  updateServicePending
}: {
  builtinServices: UnlockService[]
  customServices: UnlockService[]
  isAdmin: boolean
  onDelete: (id: string) => void
  onEdit: (service: UnlockService) => void
  onToggleBuiltin: (service: UnlockService) => void
  servicesLoading: boolean
  updateServicePending: boolean
}) {
  const { t } = useTranslation('ip-quality')

  const customColumns = useMemo<ColumnDef<UnlockService>[]>(
    () => [
      {
        accessorKey: 'name',
        header: t('settings_col_name'),
        enableSorting: false,
        cell: ({ row }) => <span className="font-medium">{row.original.name}</span>
      },
      {
        accessorKey: 'category',
        header: t('settings_col_category'),
        enableSorting: false,
        cell: ({ row }) => <span className="text-muted-foreground">{categoryLabel(row.original.category)}</span>
      },
      {
        accessorKey: 'enabled',
        header: t('settings_col_status'),
        enableSorting: false,
        cell: ({ row }) => <UnlockStatusBadge status={row.original.enabled ? 'unlocked' : 'blocked'} />
      },
      {
        id: 'actions',
        header: t('settings_col_actions'),
        enableSorting: false,
        meta: { className: 'text-right' },
        cell: ({ row }) => (
          <div className="flex justify-end">
            <DropdownMenu>
              <DropdownMenuTrigger
                aria-label={t('settings_action_more', { name: row.original.name })}
                render={<Button className="ml-auto" size="icon-sm" variant="ghost" />}
              >
                <MoreHorizontal aria-hidden="true" className="size-4" />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-36">
                <DropdownMenuItem
                  aria-label={t('settings_action_edit_aria', { name: row.original.name })}
                  disabled={!isAdmin}
                  onClick={() => onEdit(row.original)}
                >
                  <Pencil className="size-3.5" />
                  {t('settings_action_edit')}
                </DropdownMenuItem>
                <DropdownMenuItem
                  aria-label={t('settings_action_delete_aria', { name: row.original.name })}
                  disabled={!isAdmin}
                  onClick={() => onDelete(row.original.id)}
                  variant="destructive"
                >
                  <Trash2 className="size-3.5" />
                  {t('settings_action_delete')}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        )
      }
    ],
    [isAdmin, onDelete, onEdit, t]
  )

  const customTable = useReactTable({
    data: customServices,
    columns: customColumns,
    getCoreRowModel: getCoreRowModel(),
    getRowId: (row) => row.id
  })

  if (servicesLoading) {
    return (
      <div className="max-w-4xl space-y-2 p-4">
        {Array.from({ length: 4 }, (_, i) => (
          <Skeleton className="h-10" key={`skel-${i.toString()}`} />
        ))}
      </div>
    )
  }

  return (
    <ScrollArea className="h-full">
      <div className="max-w-4xl space-y-6 pb-4">
        <div className="space-y-2">
          <h2 className="font-semibold text-muted-foreground text-sm uppercase tracking-wide">
            {t('settings_builtin')}
          </h2>
          <div className="rounded-md border">
            {builtinServices.length === 0 && (
              <p className="px-4 py-3 text-muted-foreground text-sm">{t('settings_no_builtin')}</p>
            )}
            {builtinServices.map((service, index) => (
              <div
                className={`flex items-center justify-between px-4 py-2.5 ${
                  index < builtinServices.length - 1 ? 'border-b' : ''
                }`}
                key={service.id}
              >
                <div className="flex min-w-0 flex-col">
                  <span className="font-medium text-sm">{service.name}</span>
                  <span className="text-muted-foreground text-xs">{categoryLabel(service.category)}</span>
                </div>
                <Switch
                  aria-label={t(service.enabled ? 'settings_toggle_disable_aria' : 'settings_toggle_enable_aria', {
                    name: service.name
                  })}
                  checked={service.enabled}
                  disabled={!isAdmin || updateServicePending}
                  onCheckedChange={() => onToggleBuiltin(service)}
                />
              </div>
            ))}
          </div>
        </div>

        <div className="space-y-2">
          <h2 className="font-semibold text-muted-foreground text-sm uppercase tracking-wide">
            {t('settings_custom')}
          </h2>
          <DataTable className="w-full min-w-0 max-w-full" noResults={t('settings_no_custom')} table={customTable} />
        </div>
      </div>
    </ScrollArea>
  )
}
