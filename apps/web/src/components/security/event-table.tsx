import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import type { SecurityEventDto } from '@/lib/api-schema'
import { EventTypeBadge, SeverityBadge } from './severity-badge'

interface Props {
  events: SecurityEventDto[]
  hasNextPage?: boolean
  isFetchingNextPage?: boolean
  isLoading?: boolean
  onBlockSourceIp?: (event: SecurityEventDto) => void
  onFetchNextPage?: () => void
  onRowClick?: (event: SecurityEventDto) => void
  onSourceIpClick?: (ip: string) => void
}

function formatTime(value: string | null | undefined): string {
  if (!value) {
    return '—'
  }
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) {
    return value
  }
  return d.toLocaleString()
}

export function SecurityEventTable({
  events,
  hasNextPage,
  isFetchingNextPage,
  isLoading,
  onBlockSourceIp,
  onFetchNextPage,
  onRowClick,
  onSourceIpClick
}: Props) {
  const { t } = useTranslation('security')

  return (
    <div className="rounded-md border">
      <ScrollArea className="max-h-[560px]">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-[180px]">{t('table.created_at', { defaultValue: 'Time' })}</TableHead>
              <TableHead>{t('table.event_type', { defaultValue: 'Event' })}</TableHead>
              <TableHead>{t('table.severity', { defaultValue: 'Severity' })}</TableHead>
              <TableHead>{t('table.source_ip', { defaultValue: 'Source IP' })}</TableHead>
              <TableHead>{t('table.username', { defaultValue: 'User' })}</TableHead>
              <TableHead className="w-[140px]">{t('table.detector', { defaultValue: 'Detector' })}</TableHead>
              {onBlockSourceIp && <TableHead className="w-[60px]" />}
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading &&
              Array.from({ length: 5 }, (_, i) => (
                <TableRow key={`sec-skel-${i.toString()}`}>
                  <TableCell colSpan={onBlockSourceIp ? 7 : 6}>
                    <Skeleton className="h-6 w-full" />
                  </TableCell>
                </TableRow>
              ))}
            {!isLoading && events.length === 0 && (
              <TableRow>
                <TableCell className="text-center text-muted-foreground" colSpan={onBlockSourceIp ? 7 : 6}>
                  {t('table.empty', { defaultValue: 'No security events in this range.' })}
                </TableCell>
              </TableRow>
            )}
            {!isLoading &&
              events.map((event) => (
                <TableRow
                  className="cursor-pointer hover:bg-muted/40"
                  data-testid="security-event-row"
                  key={event.id}
                  onClick={() => onRowClick?.(event)}
                >
                  <TableCell className="whitespace-nowrap font-mono text-xs">{formatTime(event.created_at)}</TableCell>
                  <TableCell>
                    <EventTypeBadge eventType={event.event_type} firstSeen={event.first_seen} />
                  </TableCell>
                  <TableCell>
                    <SeverityBadge severity={event.severity} />
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    {onSourceIpClick ? (
                      <button
                        className="hover:text-primary hover:underline"
                        onClick={(e) => {
                          e.stopPropagation()
                          onSourceIpClick(event.source_ip)
                        }}
                        type="button"
                      >
                        {event.source_ip}
                      </button>
                    ) : (
                      event.source_ip
                    )}
                  </TableCell>
                  <TableCell className="font-mono text-xs">{event.username ?? '—'}</TableCell>
                  <TableCell className="text-muted-foreground text-xs">{event.detector_source}</TableCell>
                  {onBlockSourceIp && (
                    <TableCell className="text-right">
                      <DropdownMenu>
                        <DropdownMenuTrigger
                          render={
                            <Button
                              aria-label={t('table.row_actions', { defaultValue: 'Row actions' })}
                              onClick={(e) => e.stopPropagation()}
                              size="icon-sm"
                              variant="ghost"
                            />
                          }
                        >
                          <MoreHorizontal aria-hidden="true" className="size-3.5" />
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                          <DropdownMenuItem
                            onClick={(e) => {
                              e.stopPropagation()
                              onBlockSourceIp(event)
                            }}
                          >
                            <ShieldAlert aria-hidden="true" className="size-3.5" />
                            {t('table.action_block_source_ip', { defaultValue: 'Block source IP' })}
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </TableCell>
                  )}
                </TableRow>
              ))}
          </TableBody>
        </Table>
      </ScrollArea>
      {hasNextPage && onFetchNextPage && (
        <div className="flex justify-center border-t p-2">
          <Button disabled={isFetchingNextPage} onClick={onFetchNextPage} size="sm" variant="outline">
            {isFetchingNextPage
              ? t('table.loading_more', { defaultValue: 'Loading…' })
              : t('table.load_more', { defaultValue: 'Load more' })}
          </Button>
        </div>
      )}
    </div>
  )
}
