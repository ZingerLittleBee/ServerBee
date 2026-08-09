import { PencilIcon, PlusIcon, Star, TrashIcon } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useCreateDashboard, useDeleteDashboard, useUpdateDashboard } from '@/hooks/use-dashboard'
import type { Dashboard } from '@/lib/widget-types'

interface DashboardSwitcherProps {
  currentId: string
  dashboards: Dashboard[]
  isAdmin: boolean
  onSelect: (id: string) => void
}

export function DashboardSwitcher({ dashboards, currentId, onSelect, isAdmin }: DashboardSwitcherProps) {
  const { t } = useTranslation('dashboard')
  const [newDialogOpen, setNewDialogOpen] = useState(false)
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false)
  const [renameDialogOpen, setRenameDialogOpen] = useState(false)
  const [newName, setNewName] = useState('')
  const [renameName, setRenameName] = useState('')

  const createDashboard = useCreateDashboard()
  const deleteDashboard = useDeleteDashboard()
  const updateDashboard = useUpdateDashboard()

  const current = dashboards.find((d) => d.id === currentId)
  const isDefault = current?.is_default ?? false

  const handleCreate = () => {
    const name = newName.trim()
    if (!name) {
      return
    }
    createDashboard.mutate(
      { name },
      {
        onSuccess: (created) => {
          setNewDialogOpen(false)
          setNewName('')
          onSelect(created.id)
        }
      }
    )
  }

  const handleDelete = () => {
    if (!currentId || isDefault) {
      return
    }
    deleteDashboard.mutate(currentId, {
      onSuccess: () => {
        setDeleteDialogOpen(false)
        // Switch to first available dashboard after deletion
        const remaining = dashboards.filter((d) => d.id !== currentId)
        const next = remaining.find((d) => d.is_default) ?? remaining[0]
        if (next) {
          onSelect(next.id)
        }
      }
    })
  }

  const handleSetDefault = () => {
    if (!currentId || isDefault) {
      return
    }
    updateDashboard.mutate({ id: currentId, is_default: true })
  }

  const handleRename = () => {
    const name = renameName.trim()
    if (!(currentId && name) || name === current?.name) {
      setRenameDialogOpen(false)
      return
    }
    updateDashboard.mutate(
      { id: currentId, name },
      {
        onSuccess: () => {
          setRenameDialogOpen(false)
        }
      }
    )
  }

  return (
    <div className="flex items-center gap-2">
      <Select
        onValueChange={(v) => {
          if (v !== null) {
            onSelect(v)
          }
        }}
        value={currentId}
      >
        <SelectTrigger className="w-48">
          {/* Resolve the label from `dashboards` instead of Base UI's `items` lookup:
              the selected id is known (default dashboard or stored selection) before
              the dashboard list request resolves, and the lookup would otherwise fall
              back to rendering the raw id. */}
          <SelectValue placeholder={t('select_dashboard')}>{() => current?.name ?? t('select_dashboard')}</SelectValue>
        </SelectTrigger>
        <SelectContent>
          {dashboards.map((d) => (
            <SelectItem key={d.id} value={d.id}>
              <span className="flex items-center gap-1.5">
                {d.is_default && <Star className="size-3 shrink-0 text-amber-500" />}
                <span>{d.name}</span>
              </span>
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {isAdmin && !isDefault && (
        <Button
          aria-label={t('set_default')}
          onClick={handleSetDefault}
          size="icon-sm"
          title={t('set_default')}
          variant="ghost"
        >
          <Star className="size-4" />
        </Button>
      )}

      {isAdmin && current && (
        <Button
          aria-label={t('rename_dashboard')}
          onClick={() => {
            setRenameName(current.name)
            setRenameDialogOpen(true)
          }}
          size="icon-sm"
          title={t('rename_dashboard')}
          variant="ghost"
        >
          <PencilIcon className="size-4" />
        </Button>
      )}

      {isAdmin && (
        <Button
          onClick={() => {
            setNewName('')
            setNewDialogOpen(true)
          }}
          size="sm"
          variant="outline"
        >
          <PlusIcon className="mr-1 size-4" />
          {t('new_dashboard')}
        </Button>
      )}

      {isAdmin && !isDefault && (
        <Button
          aria-label={t('delete_dashboard')}
          onClick={() => setDeleteDialogOpen(true)}
          size="icon-sm"
          title={t('delete_dashboard')}
          variant="ghost"
        >
          <TrashIcon className="size-4 text-destructive" />
        </Button>
      )}

      {/* New dashboard dialog */}
      <Dialog onOpenChange={setNewDialogOpen} open={newDialogOpen}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('new_dashboard')}</DialogTitle>
          </DialogHeader>
          <Input
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                handleCreate()
              }
            }}
            placeholder={t('dashboard_name_placeholder')}
            value={newName}
          />
          <DialogFooter>
            <Button disabled={!newName.trim() || createDashboard.isPending} onClick={handleCreate}>
              {t('create')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Rename dashboard dialog */}
      <Dialog onOpenChange={setRenameDialogOpen} open={renameDialogOpen}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('rename_dashboard')}</DialogTitle>
          </DialogHeader>
          <Input
            onChange={(e) => setRenameName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                handleRename()
              }
            }}
            placeholder={t('dashboard_name_placeholder')}
            value={renameName}
          />
          <DialogFooter>
            <Button disabled={!renameName.trim() || updateDashboard.isPending} onClick={handleRename}>
              {t('rename')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete confirmation dialog */}
      <AlertDialog onOpenChange={setDeleteDialogOpen} open={deleteDialogOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('delete_dashboard')}</AlertDialogTitle>
            <AlertDialogDescription>
              {t('delete_dashboard_confirm', { name: current?.name ?? '' })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t('cancel')}</AlertDialogCancel>
            <AlertDialogAction onClick={handleDelete} variant="destructive">
              {t('delete')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
