import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { type SpaThemeSummary, useActivateSpaTheme, useDeleteSpaTheme, useSpaThemes } from '@/api/spa-themes'
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
import { Badge } from '@/components/ui/badge'
import { ActivateSpaThemeDialog } from './activate-spa-theme-dialog'
import { PreviewConfirmDialog } from './preview-confirm-dialog'
import { SpaThemeCard } from './spa-theme-card'
import { SpaThemeDetailsDrawer } from './spa-theme-details-drawer'
import { SpaThemeUploadCard } from './spa-theme-upload-card'

function reloadPage() {
  window.location.reload()
}

function showActivatedToast(message: string, reloadLabel: string) {
  toast.success(message, {
    action: { label: reloadLabel, onClick: reloadPage }
  })
}

export function CustomSpaThemeSection() {
  const { t } = useTranslation('spa-theme')
  const themes = useSpaThemes()
  const activate = useActivateSpaTheme()
  const del = useDeleteSpaTheme()

  const [pendingActivate, setPendingActivate] = useState<SpaThemeSummary | null>(null)
  const [pendingPreview, setPendingPreview] = useState<SpaThemeSummary | null>(null)
  const [pendingDelete, setPendingDelete] = useState<SpaThemeSummary | null>(null)
  const [drawer, setDrawer] = useState<SpaThemeSummary | null>(null)

  const handleActivateConfirm = () => {
    if (!pendingActivate) {
      return
    }
    const target = pendingActivate
    activate.mutate(target.uuid, {
      onSuccess: () => showActivatedToast(t('theme_activated'), t('reload'))
    })
    setPendingActivate(null)
  }

  const handleDeactivate = () => {
    activate.mutate(null, {
      onSuccess: () => showActivatedToast(t('theme_deactivated'), t('reload'))
    })
  }

  const handleDeleteConfirm = () => {
    if (!pendingDelete) {
      return
    }
    del.mutate(pendingDelete.uuid)
    setPendingDelete(null)
  }

  const handlePreviewConfirm = () => {
    if (pendingPreview) {
      window.open(`/?theme=preview:${pendingPreview.uuid}`, '_blank', 'noopener,noreferrer')
    }
    setPendingPreview(null)
  }

  return (
    <section className="mb-6 rounded-lg border-2 border-amber-300/40 bg-amber-50/30 p-4 dark:bg-amber-950/20">
      <header className="mb-3 flex items-center gap-2">
        <h2 className="font-semibold text-lg">{t('section_title')}</h2>
        <Badge variant="outline">{t('section_badge')}</Badge>
      </header>
      <p className="mb-4 text-muted-foreground text-sm">{t('section_description')}</p>

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
        <SpaThemeUploadCard />
        {(themes.data ?? []).map((th) => (
          <SpaThemeCard
            key={th.uuid}
            onActivate={() => setPendingActivate(th)}
            onDeactivate={handleDeactivate}
            onDelete={() => setPendingDelete(th)}
            onOpenDetails={() => setDrawer(th)}
            onPreview={() => setPendingPreview(th)}
            theme={th}
          />
        ))}
      </div>

      <PreviewConfirmDialog
        onConfirm={handlePreviewConfirm}
        onOpenChange={(v) => !v && setPendingPreview(null)}
        open={pendingPreview !== null}
      />

      <ActivateSpaThemeDialog
        onConfirm={handleActivateConfirm}
        onOpenChange={(v) => !v && setPendingActivate(null)}
        open={pendingActivate !== null}
        theme={pendingActivate}
      />

      <AlertDialog onOpenChange={(v) => !v && setPendingDelete(null)} open={pendingDelete !== null}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('delete_dialog.title')}</AlertDialogTitle>
            <AlertDialogDescription>
              {t('delete_dialog.description', { name: pendingDelete?.name ?? '' })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t('delete_dialog.cancel')}</AlertDialogCancel>
            <AlertDialogAction onClick={handleDeleteConfirm} variant="destructive">
              {t('delete_dialog.confirm')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <SpaThemeDetailsDrawer onClose={() => setDrawer(null)} theme={drawer} />
    </section>
  )
}

interface BannerProps {
  message?: string
}

export function CustomSpaThemeBanner({ message }: BannerProps) {
  const { t } = useTranslation('spa-theme')
  return (
    <div className="mb-4 rounded-lg border border-amber-300/60 bg-amber-50/50 p-3 text-sm dark:bg-amber-950/30">
      {message ?? t('color_brand_disabled_banner')}
    </div>
  )
}
