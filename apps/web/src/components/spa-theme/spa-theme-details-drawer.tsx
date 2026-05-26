import { Download } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { SpaThemeSummary } from '@/api/spa-themes'
import { Badge } from '@/components/ui/badge'
import { buttonVariants } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { cn, formatBytes } from '@/lib/utils'

interface Props {
  onClose: () => void
  theme: SpaThemeSummary | null
}

export function SpaThemeDetailsDrawer({ theme, onClose }: Props) {
  const { t } = useTranslation('spa-theme')

  const open = theme !== null
  const downloadUrl = theme ? `/api/settings/spa-themes/${theme.uuid}/package` : '#'

  // We render the manifest summary directly from the SpaThemeSummary fields the list endpoint returns.
  // For a future "show file list" view, fetch /api/settings/spa-themes/:uuid here.
  const manifestPreview = theme
    ? JSON.stringify(
        {
          author: theme.author,
          description: theme.description,
          manifest_id: theme.manifest_id,
          name: theme.name,
          version: theme.version
        },
        null,
        2
      )
    : ''

  return (
    <Sheet onOpenChange={(v) => !v && onClose()} open={open}>
      <SheetContent className="flex w-full max-w-md flex-col sm:w-[28rem]" side="right">
        <SheetHeader>
          <SheetTitle className="flex items-center gap-2">
            {theme?.name ?? ''}
            {theme?.is_active ? <Badge variant="default">{t('active_indicator')}</Badge> : null}
            {theme?.is_superseded && !theme.is_active ? <Badge variant="secondary">{t('superseded')}</Badge> : null}
          </SheetTitle>
          <SheetDescription>
            v{theme?.version ?? ''}
            {theme?.author ? ` · ${theme.author}` : ''}
          </SheetDescription>
        </SheetHeader>

        <ScrollArea className="min-h-0 flex-1 px-4" data-testid="spa-theme-details-scroll">
          <div className="space-y-4 pb-4">
            <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
              <dt className="text-muted-foreground">manifest_id</dt>
              <dd className="font-mono text-xs">{theme?.manifest_id ?? ''}</dd>
              <dt className="text-muted-foreground">uuid</dt>
              <dd className="break-all font-mono text-xs">{theme?.uuid ?? ''}</dd>
              <dt className="text-muted-foreground">size</dt>
              <dd>{theme ? formatBytes(theme.size_bytes) : ''}</dd>
              <dt className="text-muted-foreground">uploaded_by</dt>
              <dd>{theme?.uploaded_by ?? ''}</dd>
              <dt className="text-muted-foreground">uploaded_at</dt>
              <dd className="font-mono text-xs">{theme?.uploaded_at ?? ''}</dd>
            </dl>

            {theme?.description ? <p className="text-muted-foreground text-sm">{theme.description}</p> : null}

            <div>
              <div className="mb-1 font-medium text-sm">manifest.json</div>
              <pre className="overflow-x-auto rounded-md border bg-muted/40 p-3 font-mono text-xs">
                {manifestPreview}
              </pre>
            </div>
          </div>
        </ScrollArea>

        <div className="border-t p-4">
          <a
            className={cn(buttonVariants({ variant: 'outline' }), 'w-full', !theme && 'pointer-events-none opacity-50')}
            href={downloadUrl}
            rel="noreferrer"
          >
            <Download className="size-4" />
            {t('actions.download')}
          </a>
        </div>
      </SheetContent>
    </Sheet>
  )
}
