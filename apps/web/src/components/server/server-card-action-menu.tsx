import { MoreHorizontal, Pencil, RotateCcw } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { CAP_DEFAULT } from '@/lib/capabilities'
import { RecoverAgentDialog } from './recover-agent-dialog'
import { ServerCardEditDialog } from './server-card-edit-dialog'

interface ServerCardActionMenuProps {
  server: ServerMetrics
}

export function ServerCardActionMenu({ server }: ServerCardActionMenuProps) {
  const { t } = useTranslation(['servers'])
  const [recoverOpen, setRecoverOpen] = useState(false)
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
          <MoreHorizontal aria-hidden="true" className="size-3.5" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-fit">
          <DropdownMenuItem
            onClick={(e) => {
              e.stopPropagation()
              setEditOpen(true)
            }}
          >
            <Pencil aria-hidden="true" className="size-3.5" />
            {t('servers:detail_edit')}
          </DropdownMenuItem>
          {!server.online && (
            <DropdownMenuItem
              onClick={(e) => {
                e.stopPropagation()
                setRecoverOpen(true)
              }}
            >
              <RotateCcw aria-hidden="true" className="size-3.5" />
              {t('servers:recover_agent.title')}
            </DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      {recoverOpen && (
        <RecoverAgentDialog
          onOpenChange={setRecoverOpen}
          open={recoverOpen}
          server={{
            id: server.id,
            name: server.name,
            capabilities: server.capabilities ?? CAP_DEFAULT,
            outstanding_enrollment: server.outstanding_enrollment ?? null
          }}
        />
      )}

      {editOpen && <ServerCardEditDialog onClose={() => setEditOpen(false)} serverId={server.id} />}
    </>
  )
}
