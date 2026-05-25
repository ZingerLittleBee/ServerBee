import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { RefreshCw, ShieldOff, Trash2 } from 'lucide-react'
import { useMemo } from 'react'
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
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api-client'

type RateLimitScope = 'login' | 'register'

interface RateLimitEntry {
  blocked: boolean
  count: number
  ip: string
  max: number
  scope: RateLimitScope
  seconds_remaining: number
  window_start: string
}

interface RateLimitListResponse {
  entries: RateLimitEntry[]
  login_max: number
  register_max: number
  window_minutes: number
}

interface RateLimitResetRequest {
  ip?: string
  scope?: RateLimitScope
}

interface RateLimitResetResponse {
  cleared: number
}

export const Route = createFileRoute('/_authed/settings/rate-limits')({
  component: RateLimitsPage
})

function formatSecondsRemaining(seconds: number): string {
  if (seconds <= 0) {
    return '0s'
  }
  const minutes = Math.floor(seconds / 60)
  const remainder = seconds % 60
  if (minutes === 0) {
    return `${remainder}s`
  }
  if (remainder === 0) {
    return `${minutes}m`
  }
  return `${minutes}m ${remainder}s`
}

function formatTimestamp(iso: string): string {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) {
    return iso
  }
  return date.toLocaleString()
}

function RateLimitsPage() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()

  const { data, isLoading, isFetching, refetch } = useQuery<RateLimitListResponse>({
    queryKey: ['settings', 'rate-limits'],
    queryFn: () => api.get<RateLimitListResponse>('/api/admin/rate-limit'),
    // Auto-refresh so countdowns and new blocks show up without manual reload.
    refetchInterval: 5000
  })

  const resetMutation = useMutation({
    mutationFn: (req: RateLimitResetRequest) => api.post<RateLimitResetResponse>('/api/admin/rate-limit/reset', req),
    onSuccess: (resp) => {
      queryClient.invalidateQueries({ queryKey: ['settings', 'rate-limits'] }).catch(() => {
        // Invalidation error is non-critical
      })
      toast.success(t('rate_limit.toast_reset', { count: resp.cleared }))
    },
    onError: (err) => {
      const msg = err instanceof Error ? err.message : t('rate_limit.toast_reset_failed')
      toast.error(msg)
    }
  })

  const summary = useMemo(() => {
    if (!data) {
      return { total: 0, blocked: 0 }
    }
    return {
      total: data.entries.length,
      blocked: data.entries.filter((e) => e.blocked).length
    }
  }, [data])

  const handleResetOne = (entry: RateLimitEntry) => {
    resetMutation.mutate({ scope: entry.scope, ip: entry.ip })
  }

  const handleClearAll = () => {
    resetMutation.mutate({})
  }

  return (
    <div className="w-full min-w-0 max-w-[calc(100vw-1.5rem)] overflow-hidden sm:max-w-full">
      <div className="mb-6 flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div className="space-y-1">
          <h1 className="font-bold text-2xl">{t('rate_limit.title')}</h1>
          <p className="text-muted-foreground text-sm">
            {t('rate_limit.description', { minutes: data?.window_minutes ?? 15 })}
          </p>
          {data && (
            <p className="text-muted-foreground text-xs">
              {t('rate_limit.limits', {
                login: data.login_max,
                register: data.register_max,
                minutes: data.window_minutes
              })}
            </p>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button
            disabled={isFetching}
            onClick={() => {
              refetch().catch(() => {
                // Refetch error surfaces via query state — non-critical here.
              })
            }}
            size="sm"
            variant="outline"
          >
            <RefreshCw className={isFetching ? 'mr-2 size-4 animate-spin' : 'mr-2 size-4'} />
            {t('rate_limit.refresh')}
          </Button>
          <AlertDialog>
            <AlertDialogTrigger
              render={
                <Button
                  disabled={!data || data.entries.length === 0 || resetMutation.isPending}
                  size="sm"
                  variant="destructive"
                >
                  <Trash2 className="mr-2 size-4" />
                  {t('rate_limit.clear_all')}
                </Button>
              }
            />
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>{t('rate_limit.clear_all_confirm_title')}</AlertDialogTitle>
                <AlertDialogDescription>{t('rate_limit.clear_all_confirm_description')}</AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
                <AlertDialogAction onClick={handleClearAll}>{t('rate_limit.clear_all')}</AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        </div>
      </div>

      {data && data.entries.length > 0 && (
        <div className="mb-3 flex flex-wrap gap-2 text-sm">
          <Badge variant="secondary">{t('rate_limit.summary_total', { count: summary.total })}</Badge>
          {summary.blocked > 0 && (
            <Badge variant="destructive">{t('rate_limit.summary_blocked', { count: summary.blocked })}</Badge>
          )}
        </div>
      )}

      {isLoading && (
        <div className="space-y-2">
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
        </div>
      )}

      {!isLoading && data && data.entries.length === 0 && (
        <div className="rounded-lg border border-dashed p-12 text-center text-muted-foreground text-sm">
          {t('rate_limit.empty')}
        </div>
      )}

      {!isLoading && data && data.entries.length > 0 && (
        <ScrollArea className="w-full rounded-md border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('rate_limit.column_scope')}</TableHead>
                <TableHead>{t('rate_limit.column_ip')}</TableHead>
                <TableHead>{t('rate_limit.column_count')}</TableHead>
                <TableHead>{t('rate_limit.column_window_start')}</TableHead>
                <TableHead>{t('rate_limit.column_remaining')}</TableHead>
                <TableHead>{t('rate_limit.column_status')}</TableHead>
                <TableHead className="text-right">{t('rate_limit.column_actions')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.entries.map((entry) => (
                <TableRow key={`${entry.scope}-${entry.ip}`}>
                  <TableCell>
                    <Badge variant={entry.scope === 'register' ? 'default' : 'outline'}>
                      {entry.scope === 'register' ? t('rate_limit.scope_register') : t('rate_limit.scope_login')}
                    </Badge>
                  </TableCell>
                  <TableCell className="font-mono text-xs">{entry.ip}</TableCell>
                  <TableCell className="tabular-nums">
                    {entry.count} / {entry.max}
                  </TableCell>
                  <TableCell className="text-muted-foreground text-xs">{formatTimestamp(entry.window_start)}</TableCell>
                  <TableCell className="text-xs tabular-nums">
                    {formatSecondsRemaining(entry.seconds_remaining)}
                  </TableCell>
                  <TableCell>
                    {entry.blocked ? (
                      <Badge variant="destructive">{t('rate_limit.status_blocked')}</Badge>
                    ) : (
                      <Badge variant="secondary">{t('rate_limit.status_active')}</Badge>
                    )}
                  </TableCell>
                  <TableCell className="text-right">
                    <Button
                      disabled={resetMutation.isPending}
                      onClick={() => handleResetOne(entry)}
                      size="sm"
                      variant="outline"
                    >
                      <ShieldOff className="mr-2 size-4" />
                      {t('rate_limit.unblock')}
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </ScrollArea>
      )}
    </div>
  )
}
