import { useQuery } from '@tanstack/react-query'
import { Info } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { SettingsRow } from '@/components/settings/settings-row'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'

interface AboutInfo {
  version: string
}

const VERSION_ICON = <Info className="size-4" />
const VERSION_LOADING_META = <Skeleton className="h-4 w-20" />

export function VersionRow() {
  const { t } = useTranslation('settings')

  const { data, isLoading } = useQuery<AboutInfo>({
    queryKey: ['about'],
    queryFn: () => api.get<AboutInfo>('/api/about'),
    staleTime: Number.POSITIVE_INFINITY
  })
  const meta = useMemo(
    () => (isLoading ? VERSION_LOADING_META : <span className="font-mono">v{data?.version ?? '-'}</span>),
    [data?.version, isLoading]
  )

  return <SettingsRow icon={VERSION_ICON} meta={meta} title={t('about.version')} />
}
