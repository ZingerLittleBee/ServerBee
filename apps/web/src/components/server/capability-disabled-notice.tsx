import { ShieldAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'

// Shown in a route body (terminal, files, …) when the selected server has the
// required capability disabled, so the user sees a clear explanation instead of a
// silently failing WebSocket or request.
export function CapabilityDisabledNotice() {
  const { t } = useTranslation('common')
  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
      <ShieldAlert aria-hidden="true" className="size-10 text-muted-foreground" />
      <h2 className="font-semibold text-lg">{t('capability_disabled_title')}</h2>
      <p className="max-w-md text-muted-foreground text-sm">{t('capability_disabled_desc')}</p>
    </div>
  )
}
