import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download, Globe2, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { SettingsRow } from '@/components/settings/settings-row'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'

interface GeoIpStatus {
  file_size?: number
  installed: boolean
  source?: string
  updated_at?: string
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

function metaText(status: GeoIpStatus | undefined, t: (key: string) => string): string {
  if (!status) {
    return ''
  }
  if (!status.installed) {
    return t('geoip.not_installed')
  }
  const parts = [t('geoip.installed')]
  if (status.file_size) {
    parts.push(formatBytes(status.file_size))
  }
  if (status.updated_at) {
    parts.push(`${t('geoip.updated')} ${new Date(status.updated_at).toLocaleDateString()}`)
  }
  if (status.source === 'custom') {
    parts.push(t('geoip.custom_file'))
  }
  return parts.join(' · ')
}

export function GeoIpRow() {
  const { t } = useTranslation('settings')
  const queryClient = useQueryClient()

  const { data: status, isLoading } = useQuery<GeoIpStatus>({
    queryKey: ['geoip-status'],
    queryFn: () => api.get<GeoIpStatus>('/api/geoip/status')
  })

  const downloadMutation = useMutation({
    mutationFn: () => api.post<{ success: boolean; message: string }>('/api/geoip/download'),
    onSuccess: (data) => {
      if (data.success) {
        toast.success(data.message)
        queryClient.invalidateQueries({ queryKey: ['geoip-status'] })
      } else {
        toast.error(data.message)
      }
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('geoip.download_failed'))
    }
  })

  const installed = status?.installed ?? false
  const isCustom = status?.source === 'custom'
  const isPending = downloadMutation.isPending

  let buttonLabel = t('geoip.download')
  if (isPending) {
    buttonLabel = t('geoip.downloading')
  } else if (installed) {
    buttonLabel = t('geoip.update')
  }

  return (
    <SettingsRow
      action={
        isCustom ? null : (
          <Button disabled={isPending} onClick={() => downloadMutation.mutate()} size="sm" variant="outline">
            {installed ? (
              <RefreshCw className={`mr-1.5 size-4 ${isPending ? 'animate-spin' : ''}`} />
            ) : (
              <Download className="mr-1.5 size-4" />
            )}
            {buttonLabel}
          </Button>
        )
      }
      description={t('geoip.description')}
      icon={<Globe2 className="size-4" />}
      meta={isLoading ? <Skeleton className="h-4 w-24" /> : metaText(status, t)}
      title={t('geoip.title')}
    />
  )
}
