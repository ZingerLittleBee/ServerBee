import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { useFileMkdirMutation } from '@/hooks/use-file-api'
import { joinPath } from '@/lib/file-utils'

interface MkdirDialogProps {
  currentPath: string
  onClose: () => void
  open: boolean
  serverId: string
}

export function MkdirDialog({ serverId, currentPath, open, onClose }: MkdirDialogProps) {
  const { t } = useTranslation('file')
  const [folderName, setFolderName] = useState('')
  const mkdirMutation = useFileMkdirMutation(serverId)

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    const name = folderName.trim()
    if (name.length === 0) {
      return
    }
    const fullPath = joinPath(currentPath, name)
    mkdirMutation.mutate(
      { path: fullPath },
      {
        onSuccess: () => {
          toast.success(t('new_folder'))
          setFolderName('')
          onClose()
        },
        onError: (err) => {
          toast.error(err instanceof Error ? err.message : t('create_folder_failed'))
        }
      }
    )
  }

  const handleOpenChange = (isOpen: boolean) => {
    if (!isOpen) {
      setFolderName('')
      onClose()
    }
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('create_folder_title')}</DialogTitle>
        </DialogHeader>
        <form className="space-y-4" onSubmit={handleSubmit}>
          <Input
            aria-label={t('folder_name')}
            autoComplete="off"
            autoFocus
            name="folder-name"
            onChange={(e) => setFolderName(e.target.value)}
            placeholder={t('folder_name')}
            required
            type="text"
            value={folderName}
          />
          <p className="text-muted-foreground text-xs">
            {t('path')}: {currentPath}
          </p>
          <DialogFooter>
            <Button onClick={() => handleOpenChange(false)} type="button" variant="outline">
              {t('cancel')}
            </Button>
            <Button disabled={folderName.trim().length === 0 || mkdirMutation.isPending} type="submit">
              {t('new_folder')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
