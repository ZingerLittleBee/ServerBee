import { useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api-client'
import type { AuditListResponse } from '@/lib/api-schema'

/**
 * Firewall activity log. The shared audit-logs endpoint does not yet accept
 * an `action LIKE 'firewall_%'` filter, so we fetch a generous page and
 * filter client-side. Once the server route grows a `prefix` param we can
 * push this filter down. See `crates/server/src/router/api/audit.rs`.
 */
const FETCH_LIMIT = 200

export function FirewallActivityLog() {
  const { t } = useTranslation(['firewall', 'settings'])

  const { data, isLoading } = useQuery<AuditListResponse>({
    queryKey: ['firewall', 'activity', FETCH_LIMIT],
    queryFn: () => api.get<AuditListResponse>(`/api/audit-logs?limit=${FETCH_LIMIT}`)
  })

  const entries = useMemo(() => (data?.entries ?? []).filter((entry) => entry.action.startsWith('firewall_')), [data])

  return (
    <div className="rounded-md border">
      <ScrollArea className="max-h-[560px]">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-[200px]">{t('settings:audit.col_time')}</TableHead>
              <TableHead>{t('settings:audit.col_action')}</TableHead>
              <TableHead className="w-[120px]">{t('settings:audit.col_user')}</TableHead>
              <TableHead>{t('settings:audit.col_detail')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading &&
              Array.from({ length: 4 }, (_, i) => (
                <TableRow key={`fw-activity-skel-${i.toString()}`}>
                  <TableCell colSpan={4}>
                    <Skeleton className="h-6 w-full" />
                  </TableCell>
                </TableRow>
              ))}
            {!isLoading && entries.length === 0 && (
              <TableRow>
                <TableCell className="text-center text-muted-foreground" colSpan={4}>
                  {t('activity.empty', { defaultValue: 'No firewall activity yet.' })}
                </TableCell>
              </TableRow>
            )}
            {!isLoading &&
              entries.map((entry) => (
                <TableRow key={entry.id}>
                  <TableCell className="whitespace-nowrap font-mono text-xs">
                    {new Date(entry.created_at).toLocaleString()}
                  </TableCell>
                  <TableCell>
                    <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs">{entry.action}</span>
                  </TableCell>
                  <TableCell className="font-mono text-muted-foreground text-xs">{entry.user_id.slice(0, 8)}</TableCell>
                  <TableCell className="max-w-md truncate text-muted-foreground text-xs">
                    {entry.detail ?? '—'}
                  </TableCell>
                </TableRow>
              ))}
          </TableBody>
        </Table>
      </ScrollArea>
    </div>
  )
}
