import { createFileRoute } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

export const Route = createFileRoute('/_authed/security/')({
  component: SecurityIndexPage
})

function SecurityIndexPage() {
  const { t } = useTranslation('security')
  return (
    <div className="space-y-4 p-4">
      <h1 className="font-semibold text-2xl">{t('page_title', { defaultValue: 'Security Events' })}</h1>
      <p className="text-muted-foreground text-sm">
        {t('placeholder_overview', { defaultValue: 'Loading security overview…' })}
      </p>
    </div>
  )
}
