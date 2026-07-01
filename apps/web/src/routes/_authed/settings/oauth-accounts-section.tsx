import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link2Off } from 'lucide-react'
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
  AlertDialogTitle,
  AlertDialogTrigger
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { OAuthAccount } from '@/lib/api-schema'

export function OAuthAccountsSection() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [unlinkId, setUnlinkId] = useState<string | null>(null)

  const { data: accounts, isLoading } = useQuery<OAuthAccount[]>({
    queryKey: ['auth', 'oauth', 'accounts'],
    queryFn: () => api.get<OAuthAccount[]>('/api/auth/oauth/accounts')
  })

  const unlinkMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/auth/oauth/accounts/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['auth', 'oauth', 'accounts'] }).catch(() => undefined)
      toast.success(t('security.toast_account_unlinked'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
    }
  })

  return (
    <div className="rounded-lg border bg-card p-6">
      <h2 className="mb-4 font-semibold text-lg">{t('security.linked_accounts')}</h2>

      {isLoading && (
        <div className="space-y-2">
          <Skeleton className="h-12" />
          <Skeleton className="h-12" />
        </div>
      )}
      {!isLoading && (!accounts || accounts.length === 0) && (
        <p className="text-muted-foreground text-sm">{t('security.no_linked_accounts')}</p>
      )}
      {!isLoading && accounts && accounts.length > 0 && (
        <div className="space-y-2">
          {accounts.map((acct) => (
            <div
              className="flex flex-col gap-3 rounded-md border px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
              key={acct.id}
            >
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs uppercase">{acct.provider}</span>
                  <span className="font-medium text-sm">
                    {acct.display_name || acct.email || acct.provider_user_id}
                  </span>
                </div>
                {acct.email && acct.display_name && (
                  <p className="mt-0.5 text-muted-foreground text-xs">{acct.email}</p>
                )}
              </div>
              <AlertDialog
                onOpenChange={(open) => {
                  if (!open) {
                    setUnlinkId(null)
                  }
                }}
                open={unlinkId === acct.id}
              >
                <AlertDialogTrigger
                  onClick={() => setUnlinkId(acct.id)}
                  render={
                    <Button
                      aria-label={`${t('security.unlink')} ${acct.provider}`}
                      disabled={unlinkMutation.isPending}
                      size="sm"
                      variant="outline"
                    />
                  }
                >
                  <Link2Off className="size-3.5" />
                  {t('security.unlink')}
                </AlertDialogTrigger>
                <AlertDialogContent>
                  <AlertDialogHeader>
                    <AlertDialogTitle>{t('common:confirm_title')}</AlertDialogTitle>
                    <AlertDialogDescription>{t('common:confirm_delete_message')}</AlertDialogDescription>
                  </AlertDialogHeader>
                  <AlertDialogFooter>
                    <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
                    <AlertDialogAction
                      onClick={() => {
                        unlinkMutation.mutate(acct.id)
                        setUnlinkId(null)
                      }}
                      variant="destructive"
                    >
                      {t('security.unlink')}
                    </AlertDialogAction>
                  </AlertDialogFooter>
                </AlertDialogContent>
              </AlertDialog>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
