import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import type { IpQualitySnapshotData } from '@/lib/ip-quality-types'
import { cn } from '@/lib/utils'

interface Props {
  className?: string
  /** Latest IP quality snapshot for the server, or null when none yet. */
  ipQuality: IpQualitySnapshotData | null | undefined
  serverName: string
}

const RISK_TONE: Record<string, string> = {
  low: 'border-green-500/40 bg-green-500/10 text-green-600 dark:text-green-300',
  medium: 'border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-300',
  high: 'border-red-500/40 bg-red-500/10 text-red-600 dark:text-red-300',
  unknown: 'border-muted-foreground/30 bg-muted text-muted-foreground'
}

const IP_TYPE_LABELS: Record<string, string> = {
  residential: 'Residential',
  datacenter: 'Datacenter',
  hosting: 'Hosting',
  mobile: 'Mobile',
  isp: 'ISP',
  unknown: 'Unknown'
}

const MASKED_IP_PATTERN = /^\*+(\.\*+)*$/

/** A masked IP (`*.*.*.*`) is returned to unauthenticated viewers of a public
 *  status page — render it verbatim rather than treating it as missing data. */
function isMaskedIp(ip: string): boolean {
  return MASKED_IP_PATTERN.test(ip)
}

function FieldRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-4">
      <span className="text-muted-foreground text-xs">{label}</span>
      <span className="truncate text-right font-medium">{value}</span>
    </div>
  )
}

export function IpQualityCard({ ipQuality, serverName, className }: Props) {
  if (!ipQuality) {
    return (
      <Card className={cn('', className)} size="sm">
        <CardHeader>
          <CardTitle>{serverName}</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-muted-foreground text-sm">No IP quality data yet.</p>
        </CardContent>
      </Card>
    )
  }

  const riskLevel = ipQuality.risk_level || 'unknown'
  const riskTone = RISK_TONE[riskLevel] ?? RISK_TONE.unknown
  const ipTypeLabel = IP_TYPE_LABELS[ipQuality.ip_type] ?? ipQuality.ip_type
  const location = [ipQuality.city, ipQuality.region, ipQuality.country].filter(Boolean).join(', ')
  const asLabel = [ipQuality.asn, ipQuality.as_org].filter(Boolean).join(' · ')

  return (
    <Card className={cn('', className)} size="sm">
      <CardHeader>
        <CardTitle className="flex items-center justify-between gap-2">
          <span className="truncate">{serverName}</span>
          <Badge className={cn('border', riskTone)} variant="outline">
            {ipQuality.risk_score == null ? `Risk: ${riskLevel}` : `Risk ${ipQuality.risk_score} · ${riskLevel}`}
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <span
            className={cn('font-mono text-base', isMaskedIp(ipQuality.ip) && 'text-muted-foreground tracking-widest')}
          >
            {ipQuality.ip}
          </span>
          <Badge variant="secondary">{ipTypeLabel}</Badge>
        </div>
        {asLabel && <FieldRow label="ASN" value={asLabel} />}
        {location && <FieldRow label="Location" value={location} />}
        <div className="flex flex-wrap gap-1.5 pt-1">
          {ipQuality.is_proxy && (
            <Badge
              className="border border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-300"
              variant="outline"
            >
              Proxy
            </Badge>
          )}
          {ipQuality.is_vpn && (
            <Badge
              className="border border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-300"
              variant="outline"
            >
              VPN
            </Badge>
          )}
          {ipQuality.is_hosting && (
            <Badge className="border border-muted-foreground/30 bg-muted text-muted-foreground" variant="outline">
              Hosting
            </Badge>
          )}
        </div>
      </CardContent>
    </Card>
  )
}
