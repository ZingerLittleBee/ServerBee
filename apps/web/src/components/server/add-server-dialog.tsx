import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Copy, Plus, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
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
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { CreateEnrollmentResponse, EnrollmentSummary } from '@/lib/api-schema'
import { cn } from '@/lib/utils'

const TTL_OPTIONS = [
  { secs: 600, key: 'validity_10m' },
  { secs: 3600, key: 'validity_1h' },
  { secs: 86_400, key: 'validity_1d' }
] as const

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

export function AddServerDialog({ open, onClose }: { onClose: () => void; open: boolean }) {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()

  const [label, setLabel] = useState('')
  const [ttl, setTtl] = useState<number>(600)
  const [issued, setIssued] = useState<CreateEnrollmentResponse | null>(null)

  const { data: enrollments, isLoading } = useQuery<EnrollmentSummary[]>({
    queryKey: ['agent', 'enrollments'],
    queryFn: () => api.get<EnrollmentSummary[]>('/api/agent/enrollments')
  })

  const generateMutation = useMutation({
    mutationFn: () =>
      api.post<CreateEnrollmentResponse>('/api/agent/enrollments', {
        label: label.trim() || null,
        ttl_secs: ttl
      }),
    onSuccess: (data) => {
      setIssued(data)
      queryClient.invalidateQueries({ queryKey: ['agent', 'enrollments'] })
    },
    onError: () => toast.error(t('add_server.generate_failed'))
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/agent/enrollments/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['agent', 'enrollments'] })
      toast.success(t('add_server.deleted'))
    },
    onError: () => toast.error(t('add_server.delete_failed'))
  })

  const origin = window.location.origin
  const installCommand = issued
    ? `curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- agent --server-url '${origin}' --enrollment-code '${issued.code}'`
    : ''

  const copy = async (value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      toast.success(t('add_server.copied'))
    } catch {
      // Clipboard access denied
    }
  }

  const reset = () => {
    setIssued(null)
    setLabel('')
    setTtl(600)
  }

  const handleClose = () => {
    reset()
    onClose()
  }

  return (
    <Dialog
      onOpenChange={(isOpen) => {
        if (!isOpen) {
          handleClose()
        }
      }}
      open={open}
    >
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('add_server.title')}</DialogTitle>
        </DialogHeader>

        <DialogBody className="space-y-5">
          <p className="text-muted-foreground text-sm">{t('add_server.description')}</p>

          {issued ? (
            <div className="space-y-4 rounded-md border border-amber-500/40 bg-amber-500/5 p-4">
              <p className="text-amber-600 text-sm dark:text-amber-500">{t('add_server.code_once_warning')}</p>

              <div>
                <p className="mb-1 font-medium text-muted-foreground text-xs">{t('add_server.code_label')}</p>
                <div className="flex min-w-0 items-center gap-2">
                  <code className="min-w-0 flex-1 truncate rounded-md border bg-muted/50 px-3 py-2 font-mono text-sm">
                    {issued.code}
                  </code>
                  <Button
                    aria-label={t('add_server.copy')}
                    onClick={() => copy(issued.code)}
                    size="icon"
                    variant="outline"
                  >
                    <Copy className="size-4" />
                  </Button>
                </div>
              </div>

              <div>
                <p className="mb-1 font-medium text-muted-foreground text-xs">{t('add_server.install_command')}</p>
                <div className="flex min-w-0 items-start gap-2">
                  <code className="min-w-0 flex-1 break-all rounded-md border bg-muted/50 px-3 py-2 font-mono text-xs">
                    {installCommand}
                  </code>
                  <Button
                    aria-label={t('add_server.copy')}
                    onClick={() => copy(installCommand)}
                    size="icon"
                    variant="outline"
                  >
                    <Copy className="size-4" />
                  </Button>
                </div>
              </div>

              <div>
                <p className="mb-1 font-medium text-muted-foreground text-xs">{t('add_server.steps_title')}</p>
                <ol className="list-decimal space-y-1 pl-5 text-muted-foreground text-sm">
                  <li>{t('add_server.step1')}</li>
                  <li>{t('add_server.step2')}</li>
                  <li>{t('add_server.step3')}</li>
                </ol>
              </div>
            </div>
          ) : (
            <div className="space-y-4">
              <div className="space-y-1">
                <label className="font-medium text-sm" htmlFor="add-server-name">
                  {t('add_server.name_label')}
                </label>
                <Input
                  autoComplete="off"
                  id="add-server-name"
                  name="label"
                  onChange={(e) => setLabel(e.target.value)}
                  placeholder={t('add_server.name_placeholder')}
                  type="text"
                  value={label}
                />
                <p className="text-muted-foreground text-xs">{t('add_server.name_hint')}</p>
              </div>

              <div className="space-y-1">
                {/* biome-ignore lint/a11y/noLabelWithoutControl: label describes the segmented button group below */}
                <label className="font-medium text-sm">{t('add_server.validity_label')}</label>
                <div className="flex gap-2">
                  {TTL_OPTIONS.map((opt) => (
                    <Button
                      className="flex-1"
                      key={opt.secs}
                      onClick={() => setTtl(opt.secs)}
                      size="sm"
                      type="button"
                      variant={ttl === opt.secs ? 'default' : 'outline'}
                    >
                      {t(`add_server.${opt.key}`)}
                    </Button>
                  ))}
                </div>
              </div>
            </div>
          )}

          <div>
            <p className="mb-2 font-medium text-sm">{t('add_server.existing_title')}</p>
            {(() => {
              if (isLoading) {
                return <Skeleton className="h-16 rounded-md" />
              }
              if (!enrollments || enrollments.length === 0) {
                return <p className="text-muted-foreground text-sm">{t('add_server.empty')}</p>
              }
              return (
                <ScrollArea className="max-h-56">
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
                              {t('add_server.expires_at', {
                                date: new Date(item.expires_at).toLocaleString()
                              })}
                            </p>
                          </div>
                          <Badge variant={statusVariant(status)}>{t(`add_server.status_${status}`)}</Badge>
                          <AlertDialog>
                            <AlertDialogTrigger
                              render={
                                <Button
                                  aria-label={t('add_server.delete')}
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
                                <AlertDialogTitle>{t('add_server.delete_confirm_title')}</AlertDialogTitle>
                                <AlertDialogDescription>
                                  {t('add_server.delete_confirm_description')}
                                </AlertDialogDescription>
                              </AlertDialogHeader>
                              <AlertDialogFooter>
                                <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
                                <AlertDialogAction onClick={() => deleteMutation.mutate(item.id)} variant="destructive">
                                  {t('add_server.delete')}
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
        </DialogBody>

        <DialogFooter>
          {issued ? (
            <>
              <Button onClick={reset} variant="outline">
                {t('add_server.another')}
              </Button>
              <Button onClick={handleClose}>{t('add_server.done')}</Button>
            </>
          ) : (
            <>
              <Button onClick={handleClose} variant="outline">
                {t('common:cancel')}
              </Button>
              <Button
                className={cn(generateMutation.isPending && 'pointer-events-none opacity-70')}
                disabled={generateMutation.isPending}
                onClick={() => generateMutation.mutate()}
              >
                <Plus className="size-4" />
                {generateMutation.isPending ? t('add_server.generating') : t('add_server.generate')}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
