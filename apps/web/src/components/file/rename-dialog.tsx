import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import type { FileEntry } from '@/hooks/use-file-api'
import { useFileMoveMutation } from '@/hooks/use-file-api'

interface RenameDialogProps {
  entry: FileEntry | null
  onClose: () => void
  open: boolean
  serverId: string
}

export function RenameDialog({ serverId, entry, open, onClose }: RenameDialogProps) {
  const { t } = useTranslation('file')
  const [newName, setNewName] = useState('')
  const moveMutation = useFileMoveMutation(serverId)

  useEffect(() => {
    if (open && entry) {
      setNewName(entry.name)
    }
  }, [open, entry])

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (!entry || newName.trim().length === 0) {
      return
    }
    const parentDir = entry.path.substring(0, entry.path.lastIndexOf('/'))
    const newPath = parentDir ? `${parentDir}/${newName.trim()}` : `/${newName.trim()}`
    moveMutation.mutate(
      { from: entry.path, to: newPath },
      {
        onSuccess: () => {
          toast.success(t('rename'))
          onClose()
        },
        onError: (err) => {
          toast.error(err instanceof Error ? err.message : 'Rename failed')
        }
      }
    )
  }

  const handleOpenChange = (isOpen: boolean) => {
    if (!isOpen) {
      onClose()
    }
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('rename_title')}</DialogTitle>
        </DialogHeader>
        <form className="space-y-4" onSubmit={handleSubmit}>
          <Input
            autoFocus
            onChange={(e) => setNewName(e.target.value)}
            placeholder={t('new_name')}
            required
            type="text"
            value={newName}
          />
          <DialogFooter>
            <Button onClick={() => handleOpenChange(false)} type="button" variant="outline">
              {t('cancel')}
            </Button>
            <Button disabled={newName.trim().length === 0 || moveMutation.isPending} type="submit">
              {t('rename')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
