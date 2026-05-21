import { Link } from '@tanstack/react-router'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { AddBlockDrawer, type AddBlockInitialValues } from '@/components/firewall/add-block-drawer'
import { Button } from '@/components/ui/button'
import { useAuth } from '@/hooks/use-auth'
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
  const [blockOpen, setBlockOpen] = useState(false)
  const [blockInitial, setBlockInitial] = useState<AddBlockInitialValues | undefined>(undefined)
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

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
        <Button render={<Link params={{ serverId }} to="/security/$serverId" />} size="sm" variant="outline">
          {t('tab.view_all', { defaultValue: 'View all' })}
        </Button>
      </div>
      <SecurityEventTable
        events={events}
        isLoading={eventsQuery.isLoading}
        onBlockSourceIp={
          isAdmin
            ? (event) => {
                setBlockInitial({
                  target: event.source_ip,
                  cover_type: 'include',
                  server_ids: [serverId]
                })
                setBlockOpen(true)
              }
            : undefined
        }
        onRowClick={(event) => setActiveEvent(event)}
      />
      <SecurityEventDetailDrawer event={activeEvent} onOpenChange={(open) => !open && setActiveEvent(null)} />
      <AddBlockDrawer initialValues={blockInitial} onOpenChange={setBlockOpen} open={blockOpen} />
    </div>
  )
}
