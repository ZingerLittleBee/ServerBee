import { createFileRoute } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

export const Route = createFileRoute('/_authed/security/$serverId')({
  component: SecurityServerPage
})

function SecurityServerPage() {
  const { serverId } = Route.useParams()
  const { t } = useTranslation('security')
  return (
    <div className="space-y-4 p-4">
      <h1 className="font-semibold text-2xl">{t('per_server_title', { defaultValue: 'Server Security Events' })}</h1>
      <p className="text-muted-foreground text-sm">{serverId}</p>
    </div>
  )
}
