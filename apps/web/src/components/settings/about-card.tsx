import { useQuery } from '@tanstack/react-query'
import { Info } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'

interface AboutInfo {
  version: string
}

export function AboutCard() {
  const { t } = useTranslation('settings')

  const { data, isLoading } = useQuery<AboutInfo>({
    queryKey: ['about'],
    queryFn: () => api.get<AboutInfo>('/api/about'),
    staleTime: Number.POSITIVE_INFINITY
  })

  return (
    <div className="rounded-lg border bg-card p-6">
      <h2 className="mb-1 font-semibold text-lg">{t('about.title')}</h2>
      <p className="mb-4 text-muted-foreground text-sm">{t('about.description')}</p>

      <div className="flex items-center gap-3">
        <Info className="size-5 text-muted-foreground" />
        {isLoading ? (
          <Skeleton className="h-5 w-32" />
        ) : (
          <div>
            <p className="font-medium">{t('about.version')}</p>
            <p className="font-mono text-muted-foreground text-sm">v{data?.version ?? '-'}</p>
          </div>
        )}
      </div>
    </div>
  )
}
