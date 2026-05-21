import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
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
import { useDeleteBlock } from '@/hooks/use-firewall-blocks'

interface Props {
  blockId: string | null
  onOpenChange: (open: boolean) => void
  target: string | null
}

export function DeleteBlockDialog({ blockId, target, onOpenChange }: Props) {
  const { t } = useTranslation(['firewall', 'common'])
  const deleteMutation = useDeleteBlock()

  const handleConfirm = () => {
    if (!blockId) {
      return
    }
    deleteMutation.mutate(blockId, {
      onSuccess: () => {
        toast.success(t('toast.deleted', { defaultValue: 'Block removed' }))
        onOpenChange(false)
      },
      onError: (err) => {
        toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
      }
    })
  }

  return (
    <AlertDialog onOpenChange={onOpenChange} open={!!blockId}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t('delete.title', { defaultValue: 'Delete blocklist entry?' })}</AlertDialogTitle>
          <AlertDialogDescription>
            {t('delete.description', {
              defaultValue: 'Remove block for {{target}}? Covered agents will drop the rule.',
              target: target ?? ''
            })}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
          <AlertDialogAction disabled={deleteMutation.isPending} onClick={handleConfirm} variant="destructive">
            {deleteMutation.isPending ? t('delete.deleting', { defaultValue: 'Deleting…' }) : t('common:delete')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
