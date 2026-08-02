import { MoreHorizontal, Pencil, RotateCcw } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { CAP_DEFAULT } from '@/lib/capabilities'
import type { ServerMetrics } from '@/lib/server-catalog'
import { AgentReenrollmentDialog } from './agent-reenrollment-dialog'
import { ServerCardEditDialog } from './server-card-edit-dialog'

interface ServerCardActionMenuProps {
  server: ServerMetrics
}

export function ServerCardActionMenu({ server }: ServerCardActionMenuProps) {
  const { t } = useTranslation(['servers'])
  const [reenrollmentOpen, setReenrollmentOpen] = useState(false)
  const [editOpen, setEditOpen] = useState(false)

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              aria-label={t('servers:card_actions', { defaultValue: 'Server actions' })}
              onClick={(e) => e.stopPropagation()}
              size="icon-sm"
              variant="ghost"
            />
          }
        >
          <MoreHorizontal aria-hidden="true" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-fit">
          <DropdownMenuItem
            onClick={(e) => {
              e.stopPropagation()
              setEditOpen(true)
            }}
          >
            <Pencil aria-hidden="true" />
            {t('servers:detail_edit')}
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={(e) => {
              e.stopPropagation()
              setReenrollmentOpen(true)
            }}
          >
            <RotateCcw aria-hidden="true" />
            {t('servers:agent_reenrollment.title')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      {reenrollmentOpen && (
        <AgentReenrollmentDialog
          onOpenChange={setReenrollmentOpen}
          open={reenrollmentOpen}
          server={{
            id: server.id,
            name: server.name,
            capabilities: server.capabilities ?? CAP_DEFAULT,
            agent_authority: server.agent_authority ?? {
              outstanding_offer: server.outstanding_enrollment ?? null,
              status: server.has_token === false ? 'unclaimed' : 'claimed'
            }
          }}
        />
      )}

      {editOpen && <ServerCardEditDialog onClose={() => setEditOpen(false)} serverId={server.id} />}
    </>
  )
}
