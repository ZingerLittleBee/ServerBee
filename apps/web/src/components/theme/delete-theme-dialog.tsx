import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { useDeleteTheme, useThemeReferences } from '@/api/themes'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'

interface DeleteThemeDialogProps {
  onClose: () => void
  theme: {
    id: number
    name: string
  }
}

export function DeleteThemeDialog({ onClose, theme }: DeleteThemeDialogProps) {
  const { t } = useTranslation(['settings', 'common'])
  const { data: references, isLoading } = useThemeReferences(theme.id)
  const deleteTheme = useDeleteTheme()
  const blocked = references !== undefined && (references.admin || references.status_pages.length > 0)

  return (
    <Dialog
      onOpenChange={(open) => {
        if (!open) {
          onClose()
        }
      }}
      open
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('appearance.custom_themes.delete_title', { name: theme.name })}</DialogTitle>
        </DialogHeader>

        {isLoading && <p className="text-muted-foreground text-sm">{t('common:loading')}</p>}

        {references && !blocked && <p className="text-sm">{t('appearance.custom_themes.delete_confirm')}</p>}

        {blocked && references && (
          <div className="space-y-2 text-sm">
            <p>{t('appearance.custom_themes.delete_blocked')}</p>
            <ul className="list-disc pl-6">
              {references.admin && <li>{t('appearance.custom_themes.delete_used_admin')}</li>}
              {references.status_pages.map((page) => (
                <li key={page.id}>{t('appearance.custom_themes.delete_used_status_page', { name: page.name })}</li>
              ))}
            </ul>
          </div>
        )}

        <DialogFooter>
          <Button onClick={onClose} type="button" variant="outline">
            {t('common:cancel')}
          </Button>
          {!blocked && (
            <Button
              disabled={!references || deleteTheme.isPending}
              onClick={() => {
                deleteTheme.mutate(theme.id, {
                  onError: (error) => {
                    toast.error(error instanceof Error ? error.message : t('common:errors.operation_failed'))
                  },
                  onSuccess: () => {
                    toast.success(t('appearance.custom_themes.deleted'))
                    onClose()
                  }
                })
              }}
              type="button"
              variant="destructive"
            >
              {t('common:delete')}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
