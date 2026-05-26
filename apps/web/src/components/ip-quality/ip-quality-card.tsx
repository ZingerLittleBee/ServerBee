import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import type { PublicIpQualitySnapshot } from '@/lib/api-schema'
import type { IpQualitySnapshotData } from '@/lib/ip-quality-types'
import { cn } from '@/lib/utils'

/**
 * The card has two render modes:
 *
 * - `admin` — full admin/internal view (default; preserves all existing callers).
 *   Renders IP, ASN/AS-org, region/city, every `is_*` risk boolean, abuser score,
 *   plus country/risk/ip_type.
 * - `public` — public status page view. Renders ONLY the fields that exist on
 *   `PublicIpQualitySnapshot`: `country`, `ip_type`, `risk_score`, `risk_level`,
 *   `checked_at`. The shape itself lacks `ip`, `asn`, `as_org`, `region`, `city`,
 *   `abuse_email`, and the `is_*` booleans, so TypeScript already prevents leaking
 *   them — the variant flag drives layout/text and is defense-in-depth.
 */
interface AdminProps {
  className?: string
  /** Latest IP quality snapshot for the server, or null when none yet. */
  ipQuality: IpQualitySnapshotData | null | undefined
  serverName: string
  variant?: 'admin'
}

interface PublicProps {
  className?: string
  /** Public snapshot — null until the agent has reported once. */
  ipQuality: PublicIpQualitySnapshot | null | undefined
  serverName: string
  variant: 'public'
}

type Props = AdminProps | PublicProps

const RISK_TONE: Record<string, string> = {
  low: 'border-green-500/40 bg-green-500/10 text-green-600 dark:text-green-300',
  medium: 'border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-300',
  high: 'border-red-500/40 bg-red-500/10 text-red-600 dark:text-red-300',
  unknown: 'border-muted-foreground/30 bg-muted text-muted-foreground'
}

const IP_TYPE_KEYS = new Set(['residential', 'datacenter', 'hosting', 'mobile', 'isp', 'unknown'])

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

export function IpQualityCard(props: Props) {
  const { t } = useTranslation('ip-quality')
  const { ipQuality, serverName, className } = props
  const variant = props.variant ?? 'admin'

  if (!ipQuality) {
    return (
      <Card className={cn('', className)} size="sm">
        <CardHeader>
          <CardTitle>{serverName}</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-muted-foreground text-sm">{t('card_no_data')}</p>
        </CardContent>
      </Card>
    )
  }

  const riskLevel = ipQuality.risk_level || 'unknown'
  const riskTone = RISK_TONE[riskLevel] ?? RISK_TONE.unknown
  const ipTypeLabel = IP_TYPE_KEYS.has(ipQuality.ip_type) ? t(`ip_type_${ipQuality.ip_type}`) : ipQuality.ip_type
  const riskBadge =
    ipQuality.risk_score == null
      ? t('card_risk', { level: riskLevel })
      : t('card_risk_with_score', { score: ipQuality.risk_score, level: riskLevel })

  if (variant === 'public') {
    // Defense-in-depth: only the fields present on PublicIpQualitySnapshot
    // are accessed here. TypeScript narrows `ipQuality` accordingly.
    const country = ipQuality.country
    const checkedAt = ipQuality.checked_at
    return (
      <Card className={cn('', className)} size="sm">
        <CardHeader>
          <CardTitle className="flex items-center justify-between gap-2">
            <span className="truncate">{serverName}</span>
            <Badge className={cn('border', riskTone)} variant="outline">
              {riskBadge}
            </Badge>
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-2">
          <div className="flex items-center gap-2">
            <Badge variant="secondary">{ipTypeLabel}</Badge>
            {country && <span className="text-muted-foreground text-xs">{country}</span>}
          </div>
          <FieldRow label={t('card_checked_at')} value={new Date(checkedAt).toLocaleString()} />
        </CardContent>
      </Card>
    )
  }

  // admin variant — original full render
  const adminIp = ipQuality as IpQualitySnapshotData
  const location = [adminIp.city, adminIp.region, adminIp.country].filter(Boolean).join(', ')
  const asLabel = [adminIp.asn, adminIp.as_org].filter(Boolean).join(' · ')

  return (
    <Card className={cn('', className)} size="sm">
      <CardHeader>
        <CardTitle className="flex items-center justify-between gap-2">
          <span className="truncate">{serverName}</span>
          <Badge className={cn('border', riskTone)} variant="outline">
            {riskBadge}
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <span
            className={cn('font-mono text-base', isMaskedIp(adminIp.ip) && 'text-muted-foreground tracking-widest')}
          >
            {adminIp.ip}
          </span>
          <Badge variant="secondary">{ipTypeLabel}</Badge>
        </div>
        {asLabel && <FieldRow label={t('card_asn')} value={asLabel} />}
        {adminIp.asn_abuser_score != null && (
          <FieldRow label={t('card_asn_score')} value={String(adminIp.asn_abuser_score)} />
        )}
        {location && <FieldRow label={t('card_location')} value={location} />}
        <div className="flex flex-wrap gap-1.5 pt-1">
          {adminIp.is_proxy && (
            <Badge
              className="border border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-300"
              variant="outline"
            >
              {t('card_proxy')}
            </Badge>
          )}
          {adminIp.is_vpn && (
            <Badge
              className="border border-amber-500/40 bg-amber-500/10 text-amber-600 dark:text-amber-300"
              variant="outline"
            >
              {t('card_vpn')}
            </Badge>
          )}
          {adminIp.is_tor && (
            <Badge className="border border-red-500/40 bg-red-500/10 text-red-600 dark:text-red-300" variant="outline">
              {t('card_tor')}
            </Badge>
          )}
          {adminIp.is_abuser && (
            <Badge className="border border-red-500/40 bg-red-500/10 text-red-600 dark:text-red-300" variant="outline">
              {t('card_abuser')}
            </Badge>
          )}
          {adminIp.is_hosting && (
            <Badge className="border border-muted-foreground/30 bg-muted text-muted-foreground" variant="outline">
              {t('card_hosting')}
            </Badge>
          )}
          {adminIp.is_mobile && (
            <Badge className="border border-muted-foreground/30 bg-muted text-muted-foreground" variant="outline">
              {t('card_mobile')}
            </Badge>
          )}
        </div>
      </CardContent>
    </Card>
  )
}
