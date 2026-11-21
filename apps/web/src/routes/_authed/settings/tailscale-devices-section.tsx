import { Link } from '@tanstack/react-router'
import { AlertTriangle, ExternalLink, KeyRound, Laptop, Link2, Search, Wifi } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import {
  evaluateKeyExpiry,
  isZeroTimestamp,
  type TailnetDeviceItem,
  type TailnetOverviewResponse
} from '@/lib/tailscale-types'

interface TailscaleDevicesSectionProps {
  isAdmin: boolean
  isLoading: boolean
  overview: TailnetOverviewResponse | undefined
}

type TranslateFn = (key: string, opts?: Record<string, unknown>) => string

function formatDeviceLastSeen(
  lastSeen: string | null | undefined,
  online: boolean | null | undefined,
  t: TranslateFn
): string {
  if (online) {
    return t('overview.online')
  }
  if (isZeroTimestamp(lastSeen)) {
    return t('peers.never')
  }
  const diffSecs = Math.max(0, Math.floor((Date.now() - Date.parse(lastSeen as string)) / 1000))
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

function TailnetDeviceRow({ device, t }: { device: TailnetDeviceItem; t: TranslateFn }) {
  const keyInfo = evaluateKeyExpiry(device.expires, device.key_expiry_disabled)
  const showHostname = Boolean(device.hostname && device.hostname !== device.name)

  return (
    <TableRow>
      <TableCell>
        <div className="flex items-center gap-2">
          <span
            aria-hidden="true"
            className={`size-2 shrink-0 rounded-full ${device.online ? 'bg-emerald-500' : 'bg-muted-foreground/40'}`}
          />
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="font-medium">{device.name}</span>
              {!device.authorized && <Badge variant="destructive">{t('settings.devices.unauthorized')}</Badge>}
            </div>
            {showHostname && <div className="truncate font-mono text-muted-foreground text-xs">{device.hostname}</div>}
            {device.tags.length > 0 && (
              <div className="mt-0.5 flex flex-wrap gap-1">
                {device.tags.map((tag) => (
                  <Badge className="font-mono text-[10px]" key={tag} variant="outline">
                    {tag}
                  </Badge>
                ))}
              </div>
            )}
          </div>
        </div>
      </TableCell>

      <TableCell>
        {device.matched_server_id ? (
          <Link
            className="inline-flex items-center gap-1 font-medium text-primary text-sm hover:underline"
            params={{ id: device.matched_server_id }}
            search={{ range: 'realtime' }}
            to="/servers/$id"
          >
            <span>{device.matched_server_name || device.matched_server_id}</span>
            <ExternalLink aria-hidden="true" className="size-3.5" />
          </Link>
        ) : (
          <span className="text-muted-foreground text-xs">{t('settings.devices.unlinked')}</span>
        )}
      </TableCell>

      <TableCell className="font-mono text-xs">
        {device.addresses.length > 0 ? (
          <div className="space-y-0.5">
            {device.addresses.map((address) => (
              <div key={address}>{address}</div>
            ))}
          </div>
        ) : (
          '—'
        )}
      </TableCell>

      <TableCell className="text-muted-foreground text-xs">
        <div>{device.os || '—'}</div>
        {device.client_version && <div className="font-mono text-[11px]">{device.client_version}</div>}
      </TableCell>

      <TableCell>
        {keyInfo.state === 'disabled' && <Badge variant="secondary">{t('overview.keyExpiryDisabled')}</Badge>}
        {keyInfo.state === 'expired' && <Badge variant="destructive">{t('overview.keyExpired')}</Badge>}
        {keyInfo.state === 'expiring_soon' && (
          <Badge className="border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400" variant="outline">
            {t('overview.keyExpiresInDays', { days: keyInfo.daysRemaining ?? 0 })}
          </Badge>
        )}
        {keyInfo.state === 'valid' && (
          <span className="font-mono text-xs">
            {t('overview.keyExpiresInDays', { days: keyInfo.daysRemaining ?? 0 })}
          </span>
        )}
      </TableCell>

      <TableCell className="text-right text-muted-foreground text-xs tabular-nums">
        {formatDeviceLastSeen(device.last_seen, device.online, t)}
      </TableCell>
    </TableRow>
  )
}

export function TailscaleDevicesSection({ isAdmin, isLoading, overview }: TailscaleDevicesSectionProps) {
  const { t } = useTranslation('tailscale')
  const [search, setSearch] = useState('')

  const devices = overview?.devices ?? []
  const filteredDevices = useMemo(() => {
    const query = search.toLowerCase().trim()
    if (!query) {
      return devices
    }
    return devices.filter((device) => {
      const nameMatch = device.name.toLowerCase().includes(query)
      const hostMatch = (device.hostname ?? '').toLowerCase().includes(query)
      const addrMatch = device.addresses.some((addr) => addr.toLowerCase().includes(query))
      const serverMatch = (device.matched_server_name ?? '').toLowerCase().includes(query)
      const osMatch = (device.os ?? '').toLowerCase().includes(query)
      return nameMatch || hostMatch || addrMatch || serverMatch || osMatch
    })
  }, [devices, search])

  if (isLoading) {
    return (
      <div className="space-y-4">
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-24 w-full" />
        </div>
        <Skeleton className="h-64 w-full" />
      </div>
    )
  }

  if (!overview?.configured) {
    return (
      <div className="flex min-h-[220px] items-center justify-center rounded-lg border border-dashed p-6">
        <div className="max-w-md text-center">
          <Laptop aria-hidden="true" className="mx-auto mb-3 size-9 text-muted-foreground" />
          <p className="font-medium text-sm">{t('settings.notConfiguredTitle')}</p>
          <p className="mt-1 text-muted-foreground text-xs">
            {isAdmin ? t('settings.notConfiguredAdminDesc') : t('settings.notConfiguredMemberDesc')}
          </p>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="font-semibold text-lg">{t('settings.overviewTitle')}</h2>
          <p className="text-muted-foreground text-xs">
            {t('settings.overviewSubtitle', {
              tailnet: overview.tailnet || '—',
              provider: overview.provider
            })}
          </p>
        </div>
        <div className="relative w-full sm:w-[240px]">
          <Search
            aria-hidden="true"
            className="absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
          />
          <Input
            aria-label={t('settings.devices.searchPlaceholder')}
            className="pl-8"
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t('settings.devices.searchPlaceholder')}
            value={search}
          />
        </div>
      </div>

      {overview.error && (
        <div
          className="flex items-center gap-2 rounded-lg border border-destructive/40 bg-destructive/10 p-3 text-destructive text-xs"
          role="alert"
        >
          <AlertTriangle aria-hidden="true" className="size-4 shrink-0" />
          <span>{overview.error}</span>
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Card size="sm">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
              <Laptop aria-hidden="true" className="size-4" />
              {t('settings.stats.totalDevices')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="font-mono font-semibold text-2xl tabular-nums">{overview.total_devices}</p>
          </CardContent>
        </Card>

        <Card size="sm">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
              <Wifi aria-hidden="true" className="size-4" />
              {t('settings.stats.onlineDevices')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="font-mono font-semibold text-2xl text-emerald-600 tabular-nums dark:text-emerald-400">
              {overview.online_devices}
            </p>
          </CardContent>
        </Card>

        <Card size="sm">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
              <Link2 aria-hidden="true" className="size-4" />
              {t('settings.stats.linkedServers')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="font-mono font-semibold text-2xl tabular-nums">{overview.linked_servers}</p>
          </CardContent>
        </Card>

        <Card size="sm">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
              <KeyRound aria-hidden="true" className="size-4" />
              {t('settings.stats.expiringKeys')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p
              className={`font-mono font-semibold text-2xl tabular-nums ${
                overview.expiring_keys_count > 0 ? 'text-amber-600 dark:text-amber-400' : ''
              }`}
            >
              {overview.expiring_keys_count}
            </p>
          </CardContent>
        </Card>
      </div>

      <ScrollArea className="max-h-[540px] rounded-lg border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t('settings.devices.table.device')}</TableHead>
              <TableHead>{t('settings.devices.table.linkedServer')}</TableHead>
              <TableHead>{t('settings.devices.table.addresses')}</TableHead>
              <TableHead>{t('settings.devices.table.osVersion')}</TableHead>
              <TableHead>{t('settings.devices.table.keyExpiry')}</TableHead>
              <TableHead className="text-right">{t('settings.devices.table.lastSeen')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {filteredDevices.length === 0 ? (
              <TableRow>
                <TableCell className="py-8 text-center text-muted-foreground" colSpan={6}>
                  {search ? t('settings.devices.noFilterMatch') : t('settings.devices.empty')}
                </TableCell>
              </TableRow>
            ) : (
              filteredDevices.map((device) => <TailnetDeviceRow device={device} key={device.id} t={t} />)
            )}
          </TableBody>
        </Table>
      </ScrollArea>
    </div>
  )
}
