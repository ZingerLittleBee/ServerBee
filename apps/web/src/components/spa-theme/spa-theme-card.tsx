import { useTranslation } from 'react-i18next'
import type { SpaThemeSummary } from '@/api/spa-themes'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'

interface Props {
  onActivate: () => void
  onDeactivate: () => void
  onDelete: () => void
  onOpenDetails: () => void
  onPreview: () => void
  theme: SpaThemeSummary
}

export function SpaThemeCard({ theme, onPreview, onActivate, onDeactivate, onDelete, onOpenDetails }: Props) {
  const { t } = useTranslation('spa-theme')
  const previewUrl = theme.has_preview ? `/api/settings/spa-themes/${theme.uuid}/preview` : null

  return (
    <div className="flex flex-col overflow-hidden rounded-lg border" data-testid={`spa-theme-card-${theme.uuid}`}>
      <div className="aspect-video bg-muted">
        {previewUrl ? (
          <img alt="" className="h-full w-full object-cover" height={180} src={previewUrl} width={320} />
        ) : null}
      </div>
      <div className="flex flex-1 flex-col gap-2 p-3">
        <div className="flex items-center justify-between gap-2">
          <div className="truncate font-medium" title={theme.name}>
            {theme.name}
          </div>
          {theme.is_active ? <Badge variant="default">{t('active_indicator')}</Badge> : null}
          {theme.is_superseded && !theme.is_active ? <Badge variant="secondary">{t('superseded')}</Badge> : null}
        </div>
        <div className="text-muted-foreground text-xs">
          v{theme.version}
          {theme.author ? ` · ${theme.author}` : ''}
        </div>
        <div className="mt-auto flex flex-wrap gap-2">
          <Button onClick={onPreview} size="sm" variant="outline">
            {t('actions.preview')}
          </Button>
          {theme.is_active ? (
            <Button onClick={onDeactivate} size="sm" variant="outline">
              {t('actions.deactivate')}
            </Button>
          ) : (
            <Button onClick={onActivate} size="sm">
              {t('actions.activate')}
            </Button>
          )}
          <Button onClick={onOpenDetails} size="sm" variant="ghost">
            {t('actions.details')}
          </Button>
          <Button
            disabled={theme.is_active}
            onClick={onDelete}
            size="sm"
            title={theme.is_active ? t('delete_active_blocked') : undefined}
            variant="destructive"
          >
            {t('actions.delete')}
          </Button>
        </div>
      </div>
    </div>
  )
}
