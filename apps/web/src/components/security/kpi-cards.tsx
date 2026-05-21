import { Globe, ScanLine, ShieldAlert, User } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Card, CardContent } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { useSecurityStats } from '@/hooks/use-security-events'

interface Props {
  serverId?: string | null
  since?: string | null
  until?: string | null
}

export function SecurityKpiCards({ serverId, since, until }: Props) {
  const { t } = useTranslation('security')
  const byType = useSecurityStats({ server_id: serverId, since, until, group_by: 'event_type' })
  const byIp = useSecurityStats({ server_id: serverId, since, until, group_by: 'source_ip', limit: 5 })

  const typeCounts = useMemo(() => {
    const map: Record<string, number> = {}
    for (const bucket of byType.data ?? []) {
      map[bucket.key] = bucket.count
    }
    return map
  }, [byType.data])

  const topAttacker = byIp.data?.[0] ?? null
  const loading = byType.isLoading || byIp.isLoading

  const items = [
    {
      key: 'brute_force',
      label: t('kpi.brute_force', { defaultValue: 'Brute Force' }),
      value: typeCounts.ssh_brute_force ?? 0,
      icon: ShieldAlert,
      tone: 'text-red-600 dark:text-red-300'
    },
    {
      key: 'port_scan',
      label: t('kpi.port_scans', { defaultValue: 'Port Scans' }),
      value: typeCounts.port_scan ?? 0,
      icon: ScanLine,
      tone: 'text-orange-600 dark:text-orange-300'
    },
    {
      key: 'ssh_login',
      label: t('kpi.new_ip_logins', { defaultValue: 'New IP Logins' }),
      value: typeCounts.ssh_login ?? 0,
      icon: User,
      tone: 'text-blue-600 dark:text-blue-300'
    },
    {
      key: 'top_attacker',
      label: t('kpi.top_attacker', { defaultValue: 'Top Attacker IP' }),
      value: topAttacker ? `${topAttacker.key} (${topAttacker.count})` : t('kpi.none', { defaultValue: '—' }),
      icon: Globe,
      tone: 'text-foreground'
    }
  ]

  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
      {items.map((item) => (
        <Card key={item.key}>
          <CardContent className="flex items-center justify-between gap-3 p-4">
            <div className="space-y-1">
              <p className="text-muted-foreground text-xs uppercase tracking-wide">{item.label}</p>
              {loading ? (
                <Skeleton className="h-7 w-24" />
              ) : (
                <p className={`font-semibold text-xl ${item.tone}`}>{item.value}</p>
              )}
            </div>
            <item.icon className={`size-6 shrink-0 opacity-80 ${item.tone}`} />
          </CardContent>
        </Card>
      ))}
    </div>
  )
}
