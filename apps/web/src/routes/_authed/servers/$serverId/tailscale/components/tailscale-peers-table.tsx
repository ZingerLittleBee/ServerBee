import { Search } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { isZeroTimestamp, type TailscalePeerInfo } from '@/lib/tailscale-types'
import { formatBytes } from '@/lib/utils'

type PeerFilter = 'all' | 'online' | 'active'

interface TailscalePeersTableProps {
  peers: TailscalePeerInfo[]
}

function formatRelativeHandshake(
  timestamp: string | null | undefined,
  t: (key: string, opts?: Record<string, unknown>) => string
): string {
  if (isZeroTimestamp(timestamp)) {
    return t('peers.never')
  }
  const diffSecs = Math.max(0, Math.floor((Date.now() - Date.parse(timestamp as string)) / 1000))
  if (diffSecs < 60) {
    return t('peers.justNow')
  }
  const mins = Math.floor(diffSecs / 60)
  if (mins < 60) {
    return t('peers.minutesAgo', { count: mins })
  }
  const hours = Math.floor(mins / 60)
  if (hours < 24) {
    return t('peers.hoursAgo', { count: hours })
  }
  const days = Math.floor(hours / 24)
  return t('peers.daysAgo', { count: days })
}

function ConnectionModeBadge({
  peer,
  t
}: {
  peer: TailscalePeerInfo
  t: (key: string, opts?: Record<string, unknown>) => string
}) {
  if (peer.connection_mode === 'direct') {
    return (
      <div className="flex flex-col items-start gap-0.5">
        <Badge
          className="border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
          variant="outline"
        >
          {t('peers.mode.direct')}
        </Badge>
        {peer.cur_addr && <span className="font-mono text-[11px] text-muted-foreground">{peer.cur_addr}</span>}
      </div>
    )
  }

  if (peer.connection_mode === 'derp') {
    return (
      <Badge className="border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400" variant="outline">
        {peer.relay ? t('peers.mode.derpRegion', { region: peer.relay }) : t('peers.mode.derp')}
      </Badge>
    )
  }

  if (peer.connection_mode === 'peer_relay') {
    return (
      <Badge className="border-sky-500/30 bg-sky-500/10 text-sky-600 dark:text-sky-400" variant="outline">
        {t('peers.mode.peerRelay')}
      </Badge>
    )
  }

  return (
    <Badge className="text-muted-foreground" variant="secondary">
      {t('peers.mode.idle')}
    </Badge>
  )
}

export function TailscalePeersTable({ peers }: TailscalePeersTableProps) {
  const { t } = useTranslation('tailscale')
  const [search, setSearch] = useState('')
  const [filter, setFilter] = useState<PeerFilter>('all')

  const onlineCount = useMemo(() => peers.filter((p) => p.online).length, [peers])
  const activeCount = useMemo(() => peers.filter((p) => p.active).length, [peers])

  const filteredPeers = useMemo(() => {
    const query = search.toLowerCase().trim()
    return peers.filter((peer) => {
      if (filter === 'online' && !peer.online) {
        return false
      }
      if (filter === 'active' && !peer.active) {
        return false
      }
      if (!query) {
        return true
      }
      const hostMatch = peer.host_name.toLowerCase().includes(query)
      const dnsMatch = (peer.dns_name ?? '').toLowerCase().includes(query)
      const ipMatch = peer.tailscale_ips.some((ip) => ip.toLowerCase().includes(query))
      const osMatch = (peer.os ?? '').toLowerCase().includes(query)
      return hostMatch || dnsMatch || ipMatch || osMatch
    })
  }, [peers, search, filter])

  return (
    <div className="space-y-3">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h2 className="font-semibold text-lg">{t('peers.title')}</h2>
        <div className="flex flex-wrap items-center gap-2">
          <div className="relative">
            <Search
              aria-hidden="true"
              className="absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
            />
            <Input
              aria-label={t('peers.searchPlaceholder')}
              className="w-[220px] pl-8"
              onChange={(e) => setSearch(e.target.value)}
              placeholder={t('peers.searchPlaceholder')}
              value={search}
            />
          </div>
          <div className="flex gap-1">
            <Button onClick={() => setFilter('all')} size="sm" variant={filter === 'all' ? 'default' : 'outline'}>
              {t('peers.filter.all')} ({peers.length})
            </Button>
            <Button onClick={() => setFilter('online')} size="sm" variant={filter === 'online' ? 'default' : 'outline'}>
              {t('peers.filter.online')} ({onlineCount})
            </Button>
            <Button onClick={() => setFilter('active')} size="sm" variant={filter === 'active' ? 'default' : 'outline'}>
              {t('peers.filter.active')} ({activeCount})
            </Button>
          </div>
        </div>
      </div>

      <ScrollArea className="max-h-[520px] rounded-lg border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t('peers.table.peer')}</TableHead>
              <TableHead>{t('peers.table.os')}</TableHead>
              <TableHead>{t('peers.table.ip')}</TableHead>
              <TableHead>{t('peers.table.connection')}</TableHead>
              <TableHead className="text-right">{t('peers.table.traffic')}</TableHead>
              <TableHead className="text-right">{t('peers.table.lastHandshake')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {filteredPeers.length === 0 ? (
              <TableRow>
                <TableCell className="py-8 text-center text-muted-foreground" colSpan={6}>
                  {search || filter !== 'all' ? t('peers.noFilterMatch') : t('peers.empty')}
                </TableCell>
              </TableRow>
            ) : (
              filteredPeers.map((peer) => (
                <TableRow key={peer.id}>
                  <TableCell>
                    <div className="flex items-center gap-2">
                      <span
                        aria-hidden="true"
                        className={`size-2 shrink-0 rounded-full ${peer.online ? 'bg-emerald-500' : 'bg-muted-foreground/40'}`}
                      />
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-1.5">
                          <span className="font-medium">{peer.host_name}</span>
                          {peer.exit_node && (
                            <Badge className="text-[10px]" variant="default">
                              {t('peers.exitNodeBadge')}
                            </Badge>
                          )}
                          {!peer.exit_node && peer.exit_node_option && (
                            <Badge className="text-[10px]" variant="outline">
                              {t('peers.exitNodeOptionBadge')}
                            </Badge>
                          )}
                        </div>
                        {peer.dns_name && (
                          <div className="truncate font-mono text-muted-foreground text-xs">{peer.dns_name}</div>
                        )}
                      </div>
                    </div>
                  </TableCell>
                  <TableCell className="text-muted-foreground">{peer.os || '—'}</TableCell>
                  <TableCell className="font-mono text-xs">
                    {peer.tailscale_ips.length > 0 ? (
                      <div className="space-y-0.5">
                        {peer.tailscale_ips.map((ip) => (
                          <div key={ip}>{ip}</div>
                        ))}
                      </div>
                    ) : (
                      '—'
                    )}
                  </TableCell>
                  <TableCell>
                    <ConnectionModeBadge peer={peer} t={t} />
                  </TableCell>
                  <TableCell className="text-right font-mono text-muted-foreground text-xs tabular-nums">
                    {formatBytes(peer.rx_bytes)} / {formatBytes(peer.tx_bytes)}
                  </TableCell>
                  <TableCell className="text-right text-muted-foreground text-xs tabular-nums">
                    {formatRelativeHandshake(peer.last_handshake, t)}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </ScrollArea>
    </div>
  )
}
