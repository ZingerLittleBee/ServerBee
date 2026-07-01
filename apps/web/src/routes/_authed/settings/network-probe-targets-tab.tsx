import { type ColumnDef, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import { Lock, MoreHorizontal, Pencil, Trash2 } from 'lucide-react'
import { useCallback, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { DataTable } from '@/components/ui/data-table'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { Skeleton } from '@/components/ui/skeleton'
import {
  getNetworkProbeTypeLabel,
  getNetworkTargetDisplayLocation,
  getNetworkTargetDisplayName,
  getNetworkTargetDisplayProvider
} from '@/lib/network-i18n'
import type { NetworkProbeTarget } from '@/lib/network-types'

export function NetworkProbeTargetsTab({
  onDelete,
  onEdit,
  targets,
  targetsLoading
}: {
  onDelete: (id: string) => void
  onEdit: (target: NetworkProbeTarget) => void
  targets: NetworkProbeTarget[]
  targetsLoading: boolean
}) {
  const { t, i18n } = useTranslation('network')
  const language = i18n.resolvedLanguage ?? i18n.language

  const getProbeTypeLabel = useCallback((probeType: string) => getNetworkProbeTypeLabel(t, probeType), [t])
  const getTargetDisplayName = useCallback(
    (target: NetworkProbeTarget) => getNetworkTargetDisplayName(t, language, target),
    [t, language]
  )
  const getTargetDisplayProvider = useCallback(
    (target: NetworkProbeTarget) => getNetworkTargetDisplayProvider(t, language, target),
    [t, language]
  )
  const getTargetDisplayLocation = useCallback(
    (target: NetworkProbeTarget) => getNetworkTargetDisplayLocation(t, language, target),
    [t, language]
  )

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
        cell: ({ row }) => (
          <span className="text-muted-foreground">{getTargetDisplayProvider(row.original) || '\u2014'}</span>
        )
      },
      {
        accessorKey: 'location',
        header: () => t('target_location'),
        enableSorting: false,
        cell: ({ row }) => (
          <span className="text-muted-foreground">{getTargetDisplayLocation(row.original) || '\u2014'}</span>
        )
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
        header: () => t('target_status', { defaultValue: 'Source' }),
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
        header: () => t('target_actions', { defaultValue: 'Manage' }),
        enableSorting: false,
        meta: { className: 'text-right' },
        cell: ({ row }) =>
          !row.original.source && (
            <div className="flex justify-end">
              <DropdownMenu>
                <DropdownMenuTrigger
                  aria-label={t('target_actions_menu_aria', {
                    defaultValue: 'More actions for {{name}}',
                    name: getTargetDisplayName(row.original)
                  })}
                  render={<Button className="ml-auto" size="icon-sm" variant="ghost" />}
                >
                  <MoreHorizontal aria-hidden="true" className="size-4" />
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="w-36">
                  <DropdownMenuItem
                    aria-label={t('edit_target_aria', { defaultValue: 'Edit {{name}}', name: row.original.name })}
                    onClick={() => onEdit(row.original)}
                  >
                    <Pencil className="size-3.5" />
                    {t('edit_target')}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    aria-label={t('delete_target_aria', { defaultValue: 'Delete {{name}}', name: row.original.name })}
                    onClick={() => onDelete(row.original.id)}
                    variant="destructive"
                  >
                    <Trash2 className="size-3.5" />
                    {t('delete_target')}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          )
      }
    ],
    [t, onDelete, onEdit, getProbeTypeLabel, getTargetDisplayLocation, getTargetDisplayName, getTargetDisplayProvider]
  )

  const targetsTable = useReactTable({
    data: targets,
    columns: targetColumns,
    getCoreRowModel: getCoreRowModel(),
    getRowId: (row) => row.id
  })

  if (targetsLoading) {
    return (
      <div className="max-w-4xl space-y-2 p-4">
        {Array.from({ length: 3 }, (_, i) => (
          <Skeleton className="h-10" key={`skel-${i.toString()}`} />
        ))}
      </div>
    )
  }

  return (
    <DataTable
      className="flex h-full w-full min-w-0 max-w-full sm:max-w-4xl"
      noResults={t('no_targets')}
      table={targetsTable}
    />
  )
}
