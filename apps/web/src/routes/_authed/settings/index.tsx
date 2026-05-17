import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Copy, Plus, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { GeoIpCard } from '@/components/settings/geoip-card'
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
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { CreateEnrollmentResponse, EnrollmentSummary } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/')({
  component: SettingsPage
})

type EnrollmentStatus = 'active' | 'consumed' | 'expired'

function enrollmentStatus(item: EnrollmentSummary): EnrollmentStatus {
  if (item.consumed_at) {
    return 'consumed'
  }
  if (new Date(item.expires_at).getTime() < Date.now()) {
    return 'expired'
  }
  return 'active'
}

function statusVariant(status: EnrollmentStatus): 'default' | 'secondary' | 'destructive' {
  if (status === 'active') {
    return 'default'
  }
  if (status === 'consumed') {
    return 'secondary'
  }
  return 'destructive'
}

function SettingsPage() {
  const { t } = useTranslation('settings')
  const queryClient = useQueryClient()
  const [issuedCode, setIssuedCode] = useState<string | null>(null)

  const { data: enrollments, isLoading } = useQuery<EnrollmentSummary[]>({
    queryKey: ['agent', 'enrollments'],
    queryFn: () => api.get<EnrollmentSummary[]>('/api/agent/enrollments')
  })

  const generateMutation = useMutation({
    mutationFn: () => api.post<CreateEnrollmentResponse>('/api/agent/enrollments', {}),
    onSuccess: (data) => {
      setIssuedCode(data.code)
      queryClient.invalidateQueries({ queryKey: ['agent', 'enrollments'] })
    },
    onError: () => toast.error(t('enrollment.generate_failed'))
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/agent/enrollments/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['agent', 'enrollments'] })
      toast.success(t('enrollment.deleted'))
    },
    onError: () => toast.error(t('enrollment.delete_failed'))
  })

  const installCommand = issuedCode
    ? `curl -fsSL ${window.location.origin}/install.sh | sudo bash -s -- --server-url ${window.location.origin} --enrollment-code ${issuedCode}`
    : ''

  const copy = async (value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      toast.success(t('copied'))
    } catch {
      // Clipboard access denied
    }
  }

  return (
    <div className="w-full min-w-0 max-w-[calc(100vw-1.5rem)] overflow-hidden sm:max-w-full">
      <h1 className="mb-6 font-bold text-2xl">{t('title')}</h1>

      <div className="w-full min-w-0 max-w-xl space-y-6">
        <div className="rounded-lg border bg-card p-6">
          <h2 className="mb-1 font-semibold text-lg">{t('enrollment.title')}</h2>
          <p className="mb-4 text-muted-foreground text-sm">{t('enrollment.description')}</p>

          <Button disabled={generateMutation.isPending} onClick={() => generateMutation.mutate()}>
            <Plus className="size-4" />
            {generateMutation.isPending ? t('enrollment.generating') : t('enrollment.generate')}
          </Button>

          {issuedCode ? (
            <div className="mt-4 space-y-3 rounded-md border border-amber-500/40 bg-amber-500/5 p-4">
              <p className="text-amber-600 text-sm dark:text-amber-500">{t('enrollment.code_once_warning')}</p>

              <div>
                <p className="mb-1 font-medium text-muted-foreground text-xs">{t('enrollment.code_label')}</p>
                <div className="flex min-w-0 items-center gap-2">
                  <code className="min-w-0 flex-1 truncate rounded-md border bg-muted/50 px-3 py-2 font-mono text-sm">
                    {issuedCode}
                  </code>
                  <Button
                    aria-label={t('enrollment.copy_code')}
                    onClick={() => copy(issuedCode)}
                    size="icon"
                    variant="outline"
                  >
                    <Copy className="size-4" />
                  </Button>
                </div>
              </div>

              <div>
                <p className="mb-1 font-medium text-muted-foreground text-xs">{t('enrollment.install_command')}</p>
                <div className="flex min-w-0 items-start gap-2">
                  <code className="min-w-0 flex-1 break-all rounded-md border bg-muted/50 px-3 py-2 font-mono text-xs">
                    {installCommand}
                  </code>
                  <Button
                    aria-label={t('enrollment.copy_command')}
                    onClick={() => copy(installCommand)}
                    size="icon"
                    variant="outline"
                  >
                    <Copy className="size-4" />
                  </Button>
                </div>
              </div>
            </div>
          ) : null}

          <div className="mt-6">
            <h3 className="mb-2 font-medium text-sm">{t('enrollment.list_title')}</h3>
            {(() => {
              if (isLoading) {
                return <Skeleton className="h-20 rounded-md" />
              }
              if (!enrollments || enrollments.length === 0) {
                return <p className="text-muted-foreground text-sm">{t('enrollment.empty')}</p>
              }
              return (
                <ScrollArea className="max-h-72">
                  <ul className="space-y-2 pr-3">
                    {enrollments.map((item) => {
                      const status = enrollmentStatus(item)
                      return (
                        <li
                          className="flex min-w-0 items-center gap-3 rounded-md border bg-muted/30 px-3 py-2"
                          key={item.id}
                        >
                          <div className="min-w-0 flex-1">
                            <div className="flex items-center gap-2">
                              <code className="font-mono text-sm">{item.code_prefix}…</code>
                              {item.label ? (
                                <span className="truncate text-muted-foreground text-sm">{item.label}</span>
                              ) : null}
                            </div>
                            <p className="text-muted-foreground text-xs">
                              {t('enrollment.expires_at', {
                                date: new Date(item.expires_at).toLocaleString()
                              })}
                            </p>
                          </div>
                          <Badge variant={statusVariant(status)}>{t(`enrollment.status_${status}`)}</Badge>
                          <AlertDialog>
                            <AlertDialogTrigger
                              render={
                                <Button
                                  aria-label={t('enrollment.delete')}
                                  disabled={deleteMutation.isPending}
                                  size="icon"
                                  variant="outline"
                                >
                                  <Trash2 className="size-4" />
                                </Button>
                              }
                            />
                            <AlertDialogContent>
                              <AlertDialogHeader>
                                <AlertDialogTitle>{t('enrollment.delete_confirm_title')}</AlertDialogTitle>
                                <AlertDialogDescription>
                                  {t('enrollment.delete_confirm_description')}
                                </AlertDialogDescription>
                              </AlertDialogHeader>
                              <AlertDialogFooter>
                                <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
                                <AlertDialogAction onClick={() => deleteMutation.mutate(item.id)} variant="destructive">
                                  {t('enrollment.delete')}
                                </AlertDialogAction>
                              </AlertDialogFooter>
                            </AlertDialogContent>
                          </AlertDialog>
                        </li>
                      )
                    })}
                  </ul>
                </ScrollArea>
              )
            })()}
          </div>
        </div>

        <GeoIpCard />
      </div>
    </div>
  )
}
