import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Database, Download, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
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

function StatusDetails({ status, t }: { status: GeoIpStatus; t: (key: string) => string }) {
  return (
    <div>
      <p className="font-medium">{status.installed ? t('geoip.installed') : t('geoip.not_installed')}</p>
      {status.installed && status.source === 'custom' && (
        <p className="text-muted-foreground text-sm">{t('geoip.custom_file')}</p>
      )}
      {status.installed && status.file_size && (
        <p className="text-muted-foreground text-sm">
          {formatBytes(status.file_size)}
          {status.updated_at && ` · ${t('geoip.updated')} ${new Date(status.updated_at).toLocaleDateString()}`}
        </p>
      )}
      {!status.installed && <p className="text-muted-foreground text-sm">{t('geoip.download_prompt')}</p>}
    </div>
  )
}

function DownloadButton({
  installed,
  isPending,
  onDownload,
  t
}: {
  installed: boolean
  isPending: boolean
  onDownload: () => void
  t: (key: string) => string
}) {
  return (
    <Button disabled={isPending} onClick={onDownload} variant="outline">
      {installed ? (
        <RefreshCw className={`mr-1.5 size-4 ${isPending ? 'animate-spin' : ''}`} />
      ) : (
        <Download className="mr-1.5 size-4" />
      )}
      {isPending ? t('geoip.downloading') : null}
      {!isPending && installed ? t('geoip.update') : null}
      {isPending || installed ? null : t('geoip.download')}
    </Button>
  )
}

export function GeoIpCard() {
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

  return (
    <div className="space-y-4">
      <div className="rounded-lg border bg-card p-6">
        <h2 className="mb-1 font-semibold text-lg">{t('geoip.title')}</h2>
        <p className="mb-4 text-muted-foreground text-sm">{t('geoip.description')}</p>

        {isLoading ? (
          <div className="space-y-3">
            <Skeleton className="h-5 w-32" />
            <Skeleton className="h-4 w-48" />
          </div>
        ) : (
          <div className="space-y-4">
            <div className="flex items-center gap-3">
              <Database className="size-5 text-muted-foreground" />
              {status && <StatusDetails status={status} t={t} />}
            </div>

            {status?.source !== 'custom' && (
              <DownloadButton
                installed={status?.installed ?? false}
                isPending={downloadMutation.isPending}
                onDownload={() => downloadMutation.mutate()}
                t={t}
              />
            )}
          </div>
        )}
      </div>

      <p className="text-muted-foreground text-xs">
        {t('geoip.data_provider')}{' '}
        <a className="underline" href="https://db-ip.com" rel="noopener noreferrer" target="_blank">
          DB-IP
        </a>
        , {t('geoip.license')}{' '}
        <a
          className="underline"
          href="https://creativecommons.org/licenses/by/4.0/"
          rel="noopener noreferrer"
          target="_blank"
        >
          CC BY 4.0
        </a>
      </p>
    </div>
  )
}
