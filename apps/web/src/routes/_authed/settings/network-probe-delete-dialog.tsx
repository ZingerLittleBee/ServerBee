import { Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'

export function NetworkProbeDeleteDialog({
  onClose,
  onConfirm,
  open,
  pending
}: {
  onClose: () => void
  onConfirm: () => void
  open: boolean
  pending: boolean
}) {
  const { t } = useTranslation('network')

  return (
    <Dialog
      onOpenChange={(isOpen) => {
        if (!isOpen) {
          onClose()
        }
      }}
      open={open}
    >
      <DialogContent className="sm:max-w-sm" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{t('delete_target')}</DialogTitle>
        </DialogHeader>
        <p className="text-muted-foreground text-sm">{t('confirm_delete_target')}</p>
        <div className="flex gap-2">
          <Button disabled={pending} onClick={onConfirm} size="sm" variant="destructive">
            <Trash2 className="mr-1 size-3.5" />
            {t('delete_target')}
          </Button>
          <Button onClick={onClose} size="sm" type="button" variant="ghost">
            {t('cancel')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
