import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Smartphone, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { MobilePairDialog } from '@/components/mobile-pair-dialog'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'

interface MobileDevice {
  created_at: string
  device_name: string
  id: string
  installation_id: string
  last_used_at: string | null
}

export const Route = createFileRoute('/_authed/settings/mobile-devices')({
  component: MobileDevicesPage
})

function formatRelativeTime(dateStr: string | null): string {
  if (!dateStr) {
    return 'Never'
  }
  const date = new Date(dateStr)
  const now = new Date()
  const diffMs = now.getTime() - date.getTime()
  const diffSec = Math.floor(diffMs / 1000)

  if (diffSec < 60) {
    return 'Just now'
  }
  const diffMin = Math.floor(diffSec / 60)
  if (diffMin < 60) {
    return `${diffMin.toString()}m ago`
  }
  const diffHours = Math.floor(diffMin / 60)
  if (diffHours < 24) {
    return `${diffHours.toString()}h ago`
  }
  const diffDays = Math.floor(diffHours / 24)
  if (diffDays < 30) {
    return `${diffDays.toString()}d ago`
  }
  return date.toLocaleDateString()
}

function MobileDevicesPage() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [deleteDeviceId, setDeleteDeviceId] = useState<string | null>(null)

  const { data: devices, isLoading } = useQuery<MobileDevice[]>({
    queryKey: ['settings', 'mobile-devices'],
    queryFn: () => api.get<MobileDevice[]>('/api/mobile/auth/devices')
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/mobile/auth/devices/${id}`),
    onSuccess: () => {
      queryClient
        .invalidateQueries({
          queryKey: ['settings', 'mobile-devices']
        })
        .catch(() => {
          // Invalidation error is non-critical
        })
      toast.success(t('mobile.device_revoked'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Operation failed')
    }
  })

  const handleRefresh = () => {
    queryClient
      .invalidateQueries({
        queryKey: ['settings', 'mobile-devices']
      })
      .catch(() => {
        // Invalidation error is non-critical
      })
  }

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">{t('mobile.title')}</h1>

      <div className="max-w-2xl space-y-6">
        <div className="rounded-lg border bg-card p-6">
          <div className="mb-4 flex items-center justify-between">
            <div>
              <h2 className="font-semibold text-lg">{t('mobile.devices')}</h2>
              <p className="text-muted-foreground text-sm">{t('mobile.devices_description')}</p>
            </div>
            <MobilePairDialog onPaired={handleRefresh} />
          </div>

          {isLoading && (
            <div className="space-y-3">
              {Array.from({ length: 3 }, (_, i) => (
                <Skeleton className="h-14" key={`skeleton-${i.toString()}`} />
              ))}
            </div>
          )}
          {!isLoading && (!devices || devices.length === 0) && (
            <div className="py-8 text-center text-muted-foreground text-sm">{t('mobile.no_devices')}</div>
          )}
          {!isLoading && devices && devices.length > 0 && (
            <div className="divide-y rounded-md border">
              {devices.map((device) => (
                <div className="flex items-center justify-between px-4 py-3" key={device.id}>
                  <div className="flex items-center gap-3">
                    <Smartphone aria-hidden="true" className="size-4 text-muted-foreground" />
                    <div>
                      <p className="font-medium text-sm">{device.device_name || t('mobile.unknown_device')}</p>
                      <div className="flex gap-3 text-muted-foreground text-xs">
                        <span>
                          {t('mobile.paired')} {new Date(device.created_at).toLocaleDateString()}
                        </span>
                        <span>
                          {t('mobile.last_active')} {formatRelativeTime(device.last_used_at)}
                        </span>
                      </div>
                    </div>
                  </div>
                  <AlertDialog
                    onOpenChange={(isOpen) => {
                      if (!isOpen) {
                        setDeleteDeviceId(null)
                      }
                    }}
                    open={deleteDeviceId === device.id}
                  >
                    <AlertDialogTrigger
                      onClick={() => setDeleteDeviceId(device.id)}
                      render={
                        <Button
                          aria-label={`${t('mobile.revoke')} ${device.device_name}`}
                          disabled={deleteMutation.isPending}
                          size="sm"
                          variant="destructive"
                        />
                      }
                    >
                      <Trash2 aria-hidden="true" className="size-3.5" />
                    </AlertDialogTrigger>
                    <AlertDialogContent>
                      <AlertDialogHeader>
                        <AlertDialogTitle>{t('common:confirm_title')}</AlertDialogTitle>
                        <AlertDialogDescription>{t('mobile.revoke_confirm')}</AlertDialogDescription>
                      </AlertDialogHeader>
                      <AlertDialogFooter>
                        <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
                        <AlertDialogAction
                          onClick={() => {
                            deleteMutation.mutate(device.id)
                            setDeleteDeviceId(null)
                          }}
                          variant="destructive"
                        >
                          {t('mobile.revoke')}
                        </AlertDialogAction>
                      </AlertDialogFooter>
                    </AlertDialogContent>
                  </AlertDialog>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
