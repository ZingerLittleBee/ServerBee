import { Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { buttonVariants } from '@/components/ui/button-variants'

/**
 * App-wide fallback for unmatched routes. Registered as the router's
 * defaultNotFoundComponent so a mistyped or stale URL lands on a styled,
 * localized page with a way back home instead of TanStack Router's bare
 * unstyled "Not Found" text.
 */
export function NotFound() {
  const { t } = useTranslation('common')

  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 bg-background p-6 text-center">
      <p className="font-bold text-6xl text-muted-foreground/40 tabular-nums">404</p>
      <div className="flex flex-col gap-1.5">
        <h1 className="font-semibold text-foreground text-xl">{t('not_found.title')}</h1>
        <p className="max-w-md text-muted-foreground text-sm">{t('not_found.description')}</p>
      </div>
      <Link className={buttonVariants()} to="/">
        {t('not_found.back_home')}
      </Link>
    </div>
  )
}
