import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Database, Download, RefreshCw } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'

export const Route = createFileRoute('/_authed/settings/geoip')({
  component: GeoIpPage
})

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

function StatusDetails({ status }: { status: GeoIpStatus }) {
  return (
    <div>
      <p className="font-medium">{status.installed ? 'Installed' : 'Not Installed'}</p>
      {status.installed && status.source === 'custom' && (
        <p className="text-muted-foreground text-sm">Using custom MMDB file</p>
      )}
      {status.installed && status.file_size && (
        <p className="text-muted-foreground text-sm">
          {formatBytes(status.file_size)}
          {status.updated_at && ` · Updated ${new Date(status.updated_at).toLocaleDateString()}`}
        </p>
      )}
      {!status.installed && (
        <p className="text-muted-foreground text-sm">Download the GeoIP database to show server locations on the map</p>
      )}
    </div>
  )
}

function DownloadButton({
  installed,
  isPending,
  onDownload
}: {
  installed: boolean
  isPending: boolean
  onDownload: () => void
}) {
  return (
    <Button disabled={isPending} onClick={onDownload} variant="outline">
      {installed ? (
        <RefreshCw className={`mr-1.5 size-4 ${isPending ? 'animate-spin' : ''}`} />
      ) : (
        <Download className="mr-1.5 size-4" />
      )}
      {isPending ? 'Downloading...' : null}
      {!isPending && installed ? 'Update' : null}
      {isPending || installed ? null : 'Download'}
    </Button>
  )
}

function GeoIpPage() {
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
      toast.error(err instanceof Error ? err.message : 'Download failed')
    }
  })

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">GeoIP Database</h1>

      <div className="max-w-2xl space-y-6">
        <div className="rounded-lg border bg-card p-6">
          {isLoading ? (
            <div className="space-y-3">
              <Skeleton className="h-5 w-32" />
              <Skeleton className="h-4 w-48" />
            </div>
          ) : (
            <div className="space-y-4">
              <div className="flex items-center gap-3">
                <Database className="size-5 text-muted-foreground" />
                {status && <StatusDetails status={status} />}
              </div>

              {status?.source !== 'custom' && (
                <DownloadButton
                  installed={status?.installed ?? false}
                  isPending={downloadMutation.isPending}
                  onDownload={() => downloadMutation.mutate()}
                />
              )}
            </div>
          )}
        </div>

        <p className="text-muted-foreground text-xs">
          Data provided by{' '}
          <a className="underline" href="https://db-ip.com" rel="noopener noreferrer" target="_blank">
            DB-IP
          </a>
          , licensed under{' '}
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
    </div>
  )
}
