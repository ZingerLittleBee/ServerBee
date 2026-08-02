import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'

function EmptyShell({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed">
      <div className="text-center">{children}</div>
    </div>
  )
}

/** Nothing has ever connected: the page has no data to show yet. */
export function ServersEmptyState() {
  const { t } = useTranslation('servers')
  return (
    <EmptyShell>
      <p className="text-muted-foreground text-sm">{t('no_servers_title')}</p>
      <p className="mt-1 text-muted-foreground text-xs">{t('no_servers_description')}</p>
    </EmptyShell>
  )
}

/** Servers exist but the current query matches none of them. */
export function ServersNoResults({ onClear, query }: { onClear: () => void; query: string }) {
  const { t } = useTranslation('servers')
  return (
    <EmptyShell>
      <p className="font-medium text-foreground text-sm">{t('no_results_title', { query })}</p>
      <p className="mt-1 text-muted-foreground text-xs">{t('no_results_description')}</p>
      <Button className="mt-3" onClick={onClear} size="sm" variant="outline">
        {t('no_results_clear')}
      </Button>
    </EmptyShell>
  )
}
