import { Bot, Globe, Hand, ShieldAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Card, CardContent } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { useFirewallStats } from '@/hooks/use-firewall-blocks'

export function FirewallKpiCards() {
  const { t } = useTranslation('firewall')
  const { data, isLoading } = useFirewallStats()

  const items = [
    {
      key: 'total',
      label: t('kpi.total', { defaultValue: 'Total blocks' }),
      value: data?.total ?? 0,
      icon: ShieldAlert,
      tone: 'text-foreground'
    },
    {
      key: 'auto',
      label: t('kpi.auto', { defaultValue: 'Auto-blocked' }),
      value: data?.auto ?? 0,
      icon: Bot,
      tone: 'text-orange-600 dark:text-orange-300'
    },
    {
      key: 'manual',
      label: t('kpi.manual', { defaultValue: 'Manual blocks' }),
      value: data?.manual ?? 0,
      icon: Hand,
      tone: 'text-blue-600 dark:text-blue-300'
    },
    {
      key: 'ipv6',
      label: t('kpi.ipv6', { defaultValue: 'IPv6 blocks' }),
      value: data?.v6 ?? 0,
      icon: Globe,
      tone: 'text-muted-foreground'
    }
  ]

  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
      {items.map((item) => (
        <Card key={item.key}>
          <CardContent className="flex items-center justify-between gap-3 p-4">
            <div className="space-y-1">
              <p className="text-muted-foreground text-xs uppercase tracking-wide">{item.label}</p>
              {isLoading ? (
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
