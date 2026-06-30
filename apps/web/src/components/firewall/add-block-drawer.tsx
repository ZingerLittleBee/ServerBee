import { useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Textarea } from '@/components/ui/textarea'
import { useCreateBlock } from '@/hooks/use-firewall-blocks'
import { ApiError, api } from '@/lib/api-client'

interface ServerLite {
  id: string
  name: string
}

export interface AddBlockInitialValues {
  comment?: string
  cover_type?: 'all' | 'include' | 'exclude'
  server_ids?: string[]
  target?: string
}

interface Props {
  initialValues?: AddBlockInitialValues
  onOpenChange: (open: boolean) => void
  open: boolean
}

interface AddBlockForm {
  comment: string
  coverType: 'all' | 'include' | 'exclude'
  serverIds: string[]
  target: string
}

function getInitialForm(initialValues?: AddBlockInitialValues): AddBlockForm {
  return {
    comment: initialValues?.comment ?? '',
    coverType: initialValues?.cover_type ?? 'all',
    serverIds: initialValues?.server_ids ?? [],
    target: initialValues?.target ?? ''
  }
}

function tryExtractReason(err: unknown, fallback: string): string {
  if (err instanceof ApiError) {
    try {
      const parsed = JSON.parse(err.message)
      const reason = parsed?.error?.message ?? parsed?.error?.detail ?? parsed?.message ?? parsed?.reason
      if (typeof reason === 'string' && reason.length > 0) {
        return reason
      }
    } catch {
      // body wasn't JSON; fall through
    }
    return err.message || fallback
  }
  if (err instanceof Error) {
    return err.message || fallback
  }
  return fallback
}

export function AddBlockDrawer({ open, onOpenChange, initialValues }: Props) {
  const { t } = useTranslation(['firewall', 'common'])
  const [form, setForm] = useState(() => getInitialForm(initialValues))

  useEffect(() => {
    if (!open) {
      return
    }
    setForm(getInitialForm(initialValues))
  }, [open, initialValues])

  const { data: servers } = useQuery<ServerLite[]>({
    queryKey: ['servers', 'lite'],
    queryFn: () => api.get<ServerLite[]>('/api/servers'),
    enabled: open && form.coverType !== 'all'
  })

  const createMutation = useCreateBlock()

  const handleSubmit = () => {
    const trimmed = form.target.trim()
    if (trimmed.length === 0) {
      toast.error(t('add.target_required', { defaultValue: 'Target is required' }))
      return
    }
    createMutation.mutate(
      {
        target: trimmed,
        cover_type: form.coverType,
        server_ids: form.coverType === 'all' ? null : form.serverIds,
        comment: form.comment.trim().length > 0 ? form.comment.trim() : null
      },
      {
        onSuccess: () => {
          toast.success(t('toast.created', { defaultValue: 'Block created' }))
          onOpenChange(false)
        },
        onError: (err) => {
          toast.error(tryExtractReason(err, t('common:errors.operation_failed')))
        }
      }
    )
  }

  return (
    <Sheet onOpenChange={onOpenChange} open={open}>
      <SheetContent className="w-full sm:max-w-md" side="right">
        <SheetHeader>
          <SheetTitle>{t('add.title', { defaultValue: 'Block IP or CIDR' })}</SheetTitle>
          <SheetDescription>
            {t('add.description', {
              defaultValue: 'Push an iptables/nft drop rule to selected agents.'
            })}
          </SheetDescription>
        </SheetHeader>

        <ScrollArea className="flex-1 px-4">
          <div className="space-y-4 py-2">
            <div className="space-y-1">
              <Label htmlFor="firewall-target">{t('add.field_target', { defaultValue: 'Target (IP or CIDR)' })}</Label>
              <Input
                id="firewall-target"
                onChange={(e) => setForm((current) => ({ ...current, target: e.target.value }))}
                placeholder="203.0.113.5 or 198.51.100.0/24"
                value={form.target}
              />
            </div>

            <div className="space-y-1">
              <Label htmlFor="firewall-cover-type">{t('add.field_cover_type', { defaultValue: 'Scope' })}</Label>
              <Select
                items={{
                  all: t('add.scope_all', { defaultValue: 'All servers' }),
                  include: t('add.scope_include', { defaultValue: 'Selected servers' }),
                  exclude: t('add.scope_exclude', { defaultValue: 'All except selected' })
                }}
                onValueChange={(v) => {
                  if (v === null) {
                    return
                  }
                  const next = v as 'all' | 'exclude' | 'include'
                  setForm((current) => ({
                    ...current,
                    coverType: next,
                    serverIds: next === 'all' ? [] : current.serverIds
                  }))
                }}
                value={form.coverType}
              >
                <SelectTrigger id="firewall-cover-type">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">{t('add.scope_all', { defaultValue: 'All servers' })}</SelectItem>
                  <SelectItem value="include">
                    {t('add.scope_include', { defaultValue: 'Selected servers' })}
                  </SelectItem>
                  <SelectItem value="exclude">
                    {t('add.scope_exclude', { defaultValue: 'All except selected' })}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>

            {form.coverType !== 'all' && (
              <div className="space-y-1">
                <Label>{t('add.field_servers', { defaultValue: 'Servers' })}</Label>
                <div className="flex flex-wrap gap-2 rounded-md border p-2">
                  {servers && servers.length > 0 ? (
                    servers.map((s) => (
                      // biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element
                      <label className="flex items-center gap-1.5 text-sm" key={s.id}>
                        <Checkbox
                          checked={form.serverIds.includes(s.id)}
                          onCheckedChange={(checked) => {
                            setForm((current) => ({
                              ...current,
                              serverIds: checked
                                ? [...current.serverIds, s.id]
                                : current.serverIds.filter((id) => id !== s.id)
                            }))
                          }}
                        />
                        {s.name}
                      </label>
                    ))
                  ) : (
                    <span className="text-muted-foreground text-xs">
                      {t('add.no_servers', { defaultValue: 'No servers available' })}
                    </span>
                  )}
                </div>
              </div>
            )}

            <div className="space-y-1">
              <Label htmlFor="firewall-comment">{t('add.field_comment', { defaultValue: 'Comment (optional)' })}</Label>
              <Textarea
                id="firewall-comment"
                onChange={(e) => setForm((current) => ({ ...current, comment: e.target.value }))}
                placeholder={t('add.comment_placeholder', { defaultValue: 'Reason for blocking' })}
                rows={3}
                value={form.comment}
              />
            </div>
          </div>
        </ScrollArea>

        <div className="flex justify-end gap-2 border-t bg-muted/30 p-4">
          <Button onClick={() => onOpenChange(false)} variant="outline">
            {t('common:cancel')}
          </Button>
          <Button disabled={createMutation.isPending} onClick={handleSubmit}>
            {createMutation.isPending
              ? t('add.creating', { defaultValue: 'Creating…' })
              : t('add.submit', { defaultValue: 'Block' })}
          </Button>
        </div>
      </SheetContent>
    </Sheet>
  )
}
