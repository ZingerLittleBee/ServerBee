import type { Table } from '@tanstack/react-table'
import { LayoutGrid, ListChecks, Plus, Search, Table2, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { DataTableToolbar } from '@/components/data-table/data-table-toolbar'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import type { ServerMetrics } from '@/hooks/use-servers-ws'

export function ServersPageToolbar({
  batchDeletePending,
  cleanupPending,
  isAdmin,
  onAddServer,
  onBatchDelete,
  onCleanup,
  onSearchChange,
  onToggleSelectMode,
  onViewModeChange,
  orphanCount,
  search,
  selectedCount,
  selectMode,
  table,
  viewMode
}: {
  batchDeletePending: boolean
  cleanupPending: boolean
  isAdmin: boolean
  onAddServer: () => void
  onBatchDelete: () => void
  onCleanup: () => void
  onSearchChange: (value: string) => void
  onToggleSelectMode: () => void
  onViewModeChange: (value: 'grid' | 'table') => void
  orphanCount: number
  search: string
  selectedCount: number
  selectMode: boolean
  table: Table<ServerMetrics>
  viewMode: 'grid' | 'table'
}) {
  const { t } = useTranslation(['servers', 'common'])

  const viewToggle = (
    <ToggleGroup
      multiple={false}
      onValueChange={(value) => value.length > 0 && onViewModeChange(value[0] as 'table' | 'grid')}
      size="default"
      value={[viewMode]}
      variant="outline"
    >
      <ToggleGroupItem aria-label={t('common:a11y.table_view')} value="table">
        <Table2 className="size-4" />
      </ToggleGroupItem>
      <ToggleGroupItem aria-label={t('common:a11y.grid_view')} value="grid">
        <LayoutGrid className="size-4" />
      </ToggleGroupItem>
    </ToggleGroup>
  )

  const cleanupButton = orphanCount > 0 && (
    <AlertDialog>
      <AlertDialogTrigger
        render={
          <Button disabled={cleanupPending} size="default" variant="outline">
            {t('servers:cleanup_orphans')} ({orphanCount})
          </Button>
        }
      />
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t('servers:cleanup_confirm_title')}</AlertDialogTitle>
          <AlertDialogDescription>
            {t('servers:cleanup_confirm_description', { count: orphanCount })}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
          <AlertDialogAction onClick={onCleanup} variant="destructive">
            {t('common:delete')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )

  const batchDeleteButton = selectedCount > 0 && (
    <AlertDialog>
      <AlertDialogTrigger
        render={
          <Button disabled={batchDeletePending} size="default" variant="destructive">
            <Trash2 aria-hidden="true" className="size-3.5" />
            {t('servers:delete_selected', { count: selectedCount })}
          </Button>
        }
      />
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t('common:confirm_title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('common:confirm_delete_message')}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
          <AlertDialogAction onClick={onBatchDelete} variant="destructive">
            {t('common:delete')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )

  const selectModeButton = viewMode === 'table' && (
    <Button onClick={onToggleSelectMode} size="default" variant={selectMode ? 'secondary' : 'outline'}>
      <ListChecks aria-hidden="true" className="size-4" />
      {selectMode ? t('servers:batch_select_exit') : t('servers:batch_select')}
    </Button>
  )

  const addServerButton = isAdmin && (
    <Button onClick={onAddServer} size="default">
      <Plus className="size-4" />
      {t('add_server.button')}
    </Button>
  )

  const rowActions = (
    <>
      {viewToggle}
      {cleanupButton}
      {batchDeleteButton}
      {addServerButton}
    </>
  )

  return (
    <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center">
      <div className="relative min-w-0 flex-1 sm:max-w-sm">
        <Search className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          aria-label={t('servers:search_placeholder')}
          autoComplete="off"
          className="pl-9"
          name="search"
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder={t('servers:search_placeholder')}
          type="text"
          value={search}
        />
      </div>
      {viewMode === 'table' ? (
        <DataTableToolbar className="w-full p-0 sm:w-auto sm:flex-1" table={table} trailingActions={selectModeButton}>
          {rowActions}
        </DataTableToolbar>
      ) : (
        <div className="flex flex-wrap items-center gap-2 sm:ml-auto sm:justify-end">{rowActions}</div>
      )}
    </div>
  )
}
