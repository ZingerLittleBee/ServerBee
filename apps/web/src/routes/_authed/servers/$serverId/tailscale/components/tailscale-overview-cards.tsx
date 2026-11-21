import { AlertTriangle, Copy, Globe, KeyRound, Route as RouteIcon, Share2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  evaluateKeyExpiry,
  type KeyExpiryInfo,
  type ServerTailscaleStatusResponse,
  splitTailscaleIps
} from '@/lib/tailscale-types'

interface TailscaleOverviewCardsProps {
  data: ServerTailscaleStatusResponse
}

type TranslateFn = (key: string, opts?: Record<string, unknown>) => string

function CopyableRow({
  copyLabel,
  label,
  onCopy,
  value
}: {
  copyLabel: string
  label: string
  onCopy: (value: string) => void
  value?: string | null
}) {
  if (!value) {
    return null
  }
  return (
    <div className="flex items-center justify-between gap-2 text-xs">
      <span className="shrink-0 text-muted-foreground">{label}</span>
      <div className="flex min-w-0 items-center gap-1">
        <code className="truncate font-mono text-foreground">{value}</code>
        <Button
          aria-label={`${copyLabel} ${value}`}
          className="size-6 shrink-0"
          onClick={() => onCopy(value)}
          size="icon"
          type="button"
          variant="ghost"
        >
          <Copy className="size-3" />
        </Button>
      </div>
    </div>
  )
}

function KeyExpiryCardContent({ keyExpiry, t }: { keyExpiry: KeyExpiryInfo; t: TranslateFn }) {
  return (
    <CardContent className="space-y-1.5">
      {keyExpiry.state === 'disabled' && <Badge variant="secondary">{t('overview.keyExpiryDisabled')}</Badge>}
      {keyExpiry.state === 'expired' && <Badge variant="destructive">{t('overview.keyExpired')}</Badge>}
      {keyExpiry.state === 'expiring_soon' && (
        <Badge className="border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400" variant="outline">
          {t('overview.keyExpiresInDays', { days: keyExpiry.daysRemaining ?? 0 })}
        </Badge>
      )}
      {keyExpiry.state === 'valid' && (
        <span className="font-mono font-semibold text-base">
          {t('overview.keyExpiresInDays', { days: keyExpiry.daysRemaining ?? 0 })}
        </span>
      )}
      {keyExpiry.formattedDate && (
        <p className="text-muted-foreground text-xs">{t('overview.keyExpiresOn', { date: keyExpiry.formattedDate })}</p>
      )}
    </CardContent>
  )
}

function resolveExitLabel(exitNode: boolean | undefined, exitNodeOption: boolean | undefined, t: TranslateFn): string {
  if (exitNode) {
    return t('overview.exitNodeActive')
  }
  if (exitNodeOption) {
    return t('overview.exitNodeOption')
  }
  return t('overview.exitNodeNone')
}

function RoutingCardContent({
  exitNode,
  exitNodeOption,
  routes,
  t,
  tags
}: {
  exitNode?: boolean
  exitNodeOption?: boolean
  routes: string[]
  t: TranslateFn
  tags: string[]
}) {
  return (
    <CardContent className="space-y-1.5">
      <Badge
        className={
          exitNode ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400' : undefined
        }
        variant={exitNode ? 'default' : 'outline'}
      >
        {resolveExitLabel(exitNode, exitNodeOption, t)}
      </Badge>
      {routes.length > 0 ? (
        <div className="flex flex-wrap gap-1 pt-0.5">
          {routes.map((route) => (
            <Badge className="font-mono text-[11px]" key={route} variant="secondary">
              {route}
            </Badge>
          ))}
        </div>
      ) : (
        <p className="text-muted-foreground text-xs">{t('overview.noRoutes')}</p>
      )}
      {tags.length > 0 && (
        <div className="flex flex-wrap gap-1 pt-0.5">
          {tags.map((tag) => (
            <Badge className="font-mono text-[10px]" key={tag} variant="outline">
              {tag}
            </Badge>
          ))}
        </div>
      )}
    </CardContent>
  )
}

function BackendStateCardContent({
  backendState,
  hostname,
  isOnline,
  t,
  version
}: {
  backendState: string
  hostname?: string | null
  isOnline: boolean
  t: TranslateFn
  version?: string | null
}) {
  return (
    <CardContent className="space-y-1.5">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-mono font-semibold text-lg">{backendState}</span>
        <Badge
          className={
            isOnline
              ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
              : 'text-muted-foreground'
          }
          variant={isOnline ? 'default' : 'outline'}
        >
          {isOnline ? t('overview.online') : t('overview.offline')}
        </Badge>
      </div>
      <p className="truncate text-muted-foreground text-xs">
        {version ? t('overview.version', { version }) : t('overview.unknownVersion')}
        {hostname ? ` · ${hostname}` : ''}
      </p>
    </CardContent>
  )
}

export function TailscaleOverviewCards({ data }: TailscaleOverviewCardsProps) {
  const { t } = useTranslation('tailscale')
  const report = data.status
  const summary = report?.summary
  const splitIps = splitTailscaleIps(summary?.tailscale_ips ?? [])
  const ipv4 = data.tailscale_ipv4 ?? splitIps.ipv4
  const ipv6 = data.tailscale_ipv6 ?? splitIps.ipv6
  const dnsName = summary?.dns_name ?? data.tailscale_dns_name
  const tailnet = summary?.tailnet_name ?? data.tailscale_tailnet
  const isOnline = summary ? summary.online : data.online
  const backendState = summary?.backend_state ?? (isOnline ? 'Running' : 'Offline')
  const keyExpiry = evaluateKeyExpiry(summary?.key_expiry)
  const warnings = report?.health_warnings ?? []

  const copyToClipboard = async (value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      toast.success(t('copied'))
    } catch {
      // Clipboard access denied
    }
  }

  return (
    <div className="space-y-4">
      {warnings.length > 0 && (
        <div
          className="rounded-lg border border-amber-500/40 bg-amber-500/10 p-4 text-amber-900 dark:text-amber-200"
          role="alert"
        >
          <div className="flex items-center gap-2 font-medium text-sm">
            <AlertTriangle aria-hidden="true" className="size-4 shrink-0 text-amber-600 dark:text-amber-400" />
            <span>{t('overview.healthWarningsTitle')}</span>
          </div>
          <ul className="mt-2 list-disc space-y-1 pl-5 text-xs">
            {warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-5">
        <Card size="sm">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
              <Share2 aria-hidden="true" className="size-4" />
              {t('overview.backendState')}
            </CardTitle>
          </CardHeader>
          <BackendStateCardContent
            backendState={backendState}
            hostname={summary?.hostname}
            isOnline={isOnline}
            t={t}
            version={summary?.version}
          />
        </Card>

        <Card size="sm">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
              <Globe aria-hidden="true" className="size-4" />
              {t('overview.addresses')}
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-1">
            {!(ipv4 || ipv6 || dnsName) && <p className="text-muted-foreground text-xs">{t('overview.none')}</p>}
            <CopyableRow copyLabel={t('copy')} label={t('overview.ipv4')} onCopy={copyToClipboard} value={ipv4} />
            <CopyableRow copyLabel={t('copy')} label={t('overview.ipv6')} onCopy={copyToClipboard} value={ipv6} />
            <CopyableRow
              copyLabel={t('copy')}
              label={t('overview.magicDns')}
              onCopy={copyToClipboard}
              value={dnsName}
            />
          </CardContent>
        </Card>

        <Card size="sm">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
              <Globe aria-hidden="true" className="size-4" />
              {t('overview.tailnetAndDerp')}
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-1.5">
            <p className="truncate font-mono font-semibold text-base">{tailnet || '—'}</p>
            <p className="text-muted-foreground text-xs">
              {report?.derp_region ? t('overview.derpRegion', { region: report.derp_region }) : t('overview.noDerp')}
            </p>
          </CardContent>
        </Card>

        <Card size="sm">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
              <KeyRound aria-hidden="true" className="size-4" />
              {t('overview.keyExpiry')}
            </CardTitle>
          </CardHeader>
          <KeyExpiryCardContent keyExpiry={keyExpiry} t={t} />
        </Card>

        <Card size="sm">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
              <RouteIcon aria-hidden="true" className="size-4" />
              {t('overview.routing')}
            </CardTitle>
          </CardHeader>
          <RoutingCardContent
            exitNode={summary?.exit_node}
            exitNodeOption={summary?.exit_node_option}
            routes={report?.advertised_routes ?? []}
            t={t}
            tags={report?.tags ?? []}
          />
        </Card>
      </div>
    </div>
  )
}
