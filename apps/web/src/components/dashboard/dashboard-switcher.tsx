import { LayoutDashboard, PlusIcon, Star, TrashIcon } from 'lucide-react'
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
  const [newName, setNewName] = useState('')

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

  return (
    <div className="flex items-center gap-2">
      <LayoutDashboard className="size-5 text-muted-foreground" />
      <Select
        onValueChange={(v) => {
          if (v !== null) {
            onSelect(v)
          }
        }}
        value={currentId}
      >
        <SelectTrigger className="w-48">
          <SelectValue placeholder={t('select_dashboard')} />
        </SelectTrigger>
        <SelectContent>
          {dashboards.map((d) => (
            <SelectItem key={d.id} value={d.id}>
              {d.is_default && <Star className="mr-1 inline size-3 text-amber-500" />}
              {d.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {isAdmin && !isDefault && (
        <Button onClick={handleSetDefault} size="sm" title={t('set_default')} variant="ghost">
          <Star className="size-4" />
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
        <Button onClick={() => setDeleteDialogOpen(true)} size="sm" variant="ghost">
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
