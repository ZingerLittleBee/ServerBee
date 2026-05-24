import { useQuery } from '@tanstack/react-query'
import { Info } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { SettingsRow } from '@/components/settings/settings-row'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'

interface AboutInfo {
  version: string
}

export function VersionRow() {
  const { t } = useTranslation('settings')

  const { data, isLoading } = useQuery<AboutInfo>({
    queryKey: ['about'],
    queryFn: () => api.get<AboutInfo>('/api/about'),
    staleTime: Number.POSITIVE_INFINITY
  })

  return (
    <SettingsRow
      icon={<Info className="size-4" />}
      meta={isLoading ? <Skeleton className="h-4 w-20" /> : <span className="font-mono">v{data?.version ?? '-'}</span>}
      title={t('about.version')}
    />
  )
}
