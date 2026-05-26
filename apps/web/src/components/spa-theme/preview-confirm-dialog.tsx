import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from '@/components/ui/dialog'

interface Props {
  onConfirm: () => void
  onOpenChange: (v: boolean) => void
  open: boolean
}

export function PreviewConfirmDialog({ open, onOpenChange, onConfirm }: Props) {
  const { t } = useTranslation('spa-theme')

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('preview_dialog.title')}</DialogTitle>
          <DialogDescription>{t('preview_dialog.body')}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button onClick={() => onOpenChange(false)} variant="outline">
            {t('preview_dialog.cancel')}
          </Button>
          <Button onClick={onConfirm}>{t('preview_dialog.confirm')}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
