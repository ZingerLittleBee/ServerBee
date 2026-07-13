import { useMutation, useQueryClient } from '@tanstack/react-query'
import { MoreHorizontal, RefreshCw, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { api } from '@/lib/api-client'
import type { OutstandingEnrollmentSummary } from '@/lib/api-schema'
import { projectServerCatalog } from '@/lib/server-catalog'
import { EnrollmentOfferDialog } from './enrollment-offer-dialog'

interface PendingActionMenuProps {
  outstandingOffer: OutstandingEnrollmentSummary | null
  serverId: string
  serverName: string
}

export function PendingActionMenu({ outstandingOffer, serverId, serverName }: PendingActionMenuProps) {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()
  const [offerDialogOpen, setOfferDialogOpen] = useState(false)
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false)

  const deleteMutation = useMutation({
    mutationFn: () => api.delete<void>(`/api/servers/${serverId}`),
    onSuccess: () => {
      projectServerCatalog(queryClient, { kind: 'servers_removed', serverIds: [serverId] })
      toast.success(t('servers:card_pending.deleted'))
      setConfirmDeleteOpen(false)
    },
    onError: (err: unknown) => {
      toast.error(err instanceof Error ? err.message : t('servers:card_pending.delete_failed'))
    }
  })

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              aria-label={`${t('servers:card_pending.issue_offer')} / ${t('servers:card_pending.delete_server')}`}
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
              setOfferDialogOpen(true)
            }}
          >
            <RefreshCw aria-hidden="true" className="size-3.5" />
            {t('servers:card_pending.issue_offer')}
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={(e) => {
              e.stopPropagation()
              setConfirmDeleteOpen(true)
            }}
          >
            <Trash2 aria-hidden="true" className="size-3.5" />
            {t('servers:card_pending.delete_server')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <EnrollmentOfferDialog
        onOpenChange={setOfferDialogOpen}
        open={offerDialogOpen}
        outstandingOffer={outstandingOffer}
        serverId={serverId}
      />

      <AlertDialog onOpenChange={setConfirmDeleteOpen} open={confirmDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('servers:card_pending.delete_confirm_title')}</AlertDialogTitle>
            <AlertDialogDescription>
              {t('servers:card_pending.delete_confirm_description', { name: serverName })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleteMutation.isPending}>{t('common:cancel')}</AlertDialogCancel>
            <AlertDialogAction
              disabled={deleteMutation.isPending}
              onClick={() => deleteMutation.mutate()}
              variant="destructive"
            >
              {t('common:delete')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
