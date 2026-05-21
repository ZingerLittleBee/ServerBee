import { Link } from '@tanstack/react-router'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { useSecurityEvents } from '@/hooks/use-security-events'
import type { SecurityEventDto } from '@/lib/api-schema'
import { SecurityEventDetailDrawer } from './event-detail-drawer'
import { SecurityEventTable } from './event-table'

interface Props {
  serverId: string
}

export function ServerSecurityTab({ serverId }: Props) {
  const { t } = useTranslation('security')
  const [activeEvent, setActiveEvent] = useState<SecurityEventDto | null>(null)

  const eventsQuery = useSecurityEvents({ server_id: serverId, limit: 50 })

  const events = useMemo(() => {
    const out: SecurityEventDto[] = []
    for (const page of eventsQuery.data?.pages ?? []) {
      for (const item of page.items) {
        out.push(item)
      }
    }
    return out.slice(0, 50)
  }, [eventsQuery.data])

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <p className="text-muted-foreground text-sm">
          {t('tab.recent_label', { defaultValue: 'Most recent 50 security events on this server' })}
        </p>
        <Button asChild size="sm" variant="outline">
          <Link params={{ serverId }} to="/security/$serverId">
            {t('tab.view_all', { defaultValue: 'View all' })}
          </Link>
        </Button>
      </div>
      <SecurityEventTable
        events={events}
        isLoading={eventsQuery.isLoading}
        onRowClick={(event) => setActiveEvent(event)}
      />
      <SecurityEventDetailDrawer event={activeEvent} onOpenChange={(open) => !open && setActiveEvent(null)} />
    </div>
  )
}
