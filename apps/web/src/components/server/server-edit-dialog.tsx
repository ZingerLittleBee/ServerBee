import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CalendarIcon } from 'lucide-react'
import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Calendar } from '@/components/ui/calendar'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useServerTags, useUpdateServerTags } from '@/hooks/use-server-tags'
import { api } from '@/lib/api-client'
import type { ServerGroup, ServerResponse, UpdateServerInput } from '@/lib/api-schema'

const TAG_SPLIT_RE = /[\s,]+/
const TAG_VALID_RE = /^[A-Za-z0-9_.-]+$/

function formatIsoDate(date: Date): string {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}

function parseTagsInput(raw: string): { tags: string[]; error: string | null } {
  const parts = raw
    .split(TAG_SPLIT_RE)
    .map((t) => t.trim())
    .filter(Boolean)
  const seen = new Set<string>()
  const deduped: string[] = []
  for (const tag of parts) {
    if (tag.length > 16) {
      return { tags: [], error: 'tags_validation_too_long' }
    }
    if (!TAG_VALID_RE.test(tag)) {
      return { tags: [], error: 'tags_validation_invalid_char' }
    }
    if (seen.has(tag)) {
      continue
    }
    seen.add(tag)
    deduped.push(tag)
  }
  if (deduped.length > 8) {
    return { tags: [], error: 'tags_validation_too_many' }
  }
  return { tags: deduped.sort(), error: null }
}

interface ServerEditDialogProps {
  onClose: () => void
  open: boolean
  server: ServerResponse
}

export function ServerEditDialog({ server, open, onClose }: ServerEditDialogProps) {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()
  const [name, setName] = useState(server.name)
  const [weight, setWeight] = useState(server.weight)
  const [hidden, setHidden] = useState(server.hidden)
  const [groupId, setGroupId] = useState(server.group_id ?? '')
  const [remark, setRemark] = useState(server.remark ?? '')
  const [publicRemark, setPublicRemark] = useState(server.public_remark ?? '')
  const [price, setPrice] = useState(server.price?.toString() ?? '')
  const [billingCycle, setBillingCycle] = useState(server.billing_cycle ?? '')
  const [currency, setCurrency] = useState(server.currency ?? 'USD')
  const [expiredAt, setExpiredAt] = useState(server.expired_at?.slice(0, 10) ?? '')
  const [trafficLimit, setTrafficLimit] = useState(
    server.traffic_limit ? (server.traffic_limit / 1024 ** 3).toString() : ''
  )
  const [trafficLimitType, setTrafficLimitType] = useState(server.traffic_limit_type ?? 'sum')
  const [billingStartDay, setBillingStartDay] = useState(server.billing_start_day?.toString() ?? '')
  const [tagsInput, setTagsInput] = useState('')
  const [tagsDirty, setTagsDirty] = useState(false)

  const { data: groups } = useQuery<ServerGroup[]>({
    queryKey: ['server-groups'],
    queryFn: () => api.get<ServerGroup[]>('/api/server-groups'),
    staleTime: 60_000,
    enabled: open
  })

  const { data: initialTags } = useServerTags(server.id, open)
  const tagsMutation = useUpdateServerTags(server.id)

  useEffect(() => {
    if (open) {
      setName(server.name)
      setWeight(server.weight)
      setHidden(server.hidden)
      setGroupId(server.group_id ?? '')
      setRemark(server.remark ?? '')
      setPublicRemark(server.public_remark ?? '')
      setPrice(server.price?.toString() ?? '')
      setBillingCycle(server.billing_cycle ?? '')
      setCurrency(server.currency ?? 'USD')
      setExpiredAt(server.expired_at?.slice(0, 10) ?? '')
      setTrafficLimit(server.traffic_limit ? (server.traffic_limit / 1024 ** 3).toString() : '')
      setTrafficLimitType(server.traffic_limit_type ?? 'sum')
      setBillingStartDay(server.billing_start_day?.toString() ?? '')
    }
  }, [open, server])

  useEffect(() => {
    if (open && initialTags) {
      setTagsInput(initialTags.join(', '))
      setTagsDirty(false)
    }
  }, [open, initialTags])

  const mutation = useMutation({
    mutationFn: (payload: UpdateServerInput) => api.put<ServerResponse>(`/api/servers/${server.id}`, payload),
    onSuccess: (data) => {
      queryClient.setQueryData(['servers', server.id], data)
      queryClient.invalidateQueries({ queryKey: ['servers'] })
    }
  })

  const buildPayload = (): UpdateServerInput => ({
    name,
    weight,
    hidden,
    group_id: groupId || null,
    remark: remark || null,
    public_remark: publicRemark || null,
    price: price ? Number.parseFloat(price) : null,
    billing_cycle: billingCycle || null,
    currency: currency || null,
    expired_at: expiredAt ? `${expiredAt}T00:00:00Z` : null,
    traffic_limit: trafficLimit ? Math.round(Number.parseFloat(trafficLimit) * 1024 ** 3) : null,
    traffic_limit_type: trafficLimitType || null,
    billing_start_day: billingStartDay ? Number.parseInt(billingStartDay, 10) : null
  })

  const saveTags = async (tags: string[]): Promise<boolean> => {
    try {
      await tagsMutation.mutateAsync(tags)
      return true
    } catch (err) {
      if (initialTags) {
        setTagsInput(initialTags.join(', '))
      }
      toast.error(err instanceof Error ? err.message : t('tags_save_failed'))
      return false
    }
  }

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    const parsed = parseTagsInput(tagsInput)
    if (parsed.error) {
      toast.error(t(parsed.error))
      return
    }
    try {
      await mutation.mutateAsync(buildPayload())
    } catch (err) {
      toast.error(err instanceof Error ? err.message : t('edit_failed'))
      return
    }
    if (tagsDirty && !(await saveTags(parsed.tags))) {
      return
    }
    toast.success(t('edit_success', { defaultValue: 'Server updated successfully' }))
    onClose()
  }

  return (
    <Dialog
      onOpenChange={(isOpen) => {
        if (!isOpen) {
          onClose()
        }
      }}
      open={open}
    >
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-lg" style={{ overscrollBehavior: 'contain' }}>
        <DialogHeader>
          <DialogTitle>{t('edit_title')}</DialogTitle>
        </DialogHeader>

        <form className="space-y-4" onSubmit={handleSubmit}>
          {/* Basic */}
          <fieldset className="space-y-3">
            <legend className="mb-1 font-medium text-muted-foreground text-xs uppercase tracking-wider">
              {t('edit_basic')}
            </legend>
            <Field label={t('edit_name')}>
              <Input
                aria-label={t('edit_name')}
                name="name"
                onChange={(e) => setName(e.target.value)}
                required
                type="text"
                value={name}
              />
            </Field>
            <div className="grid grid-cols-2 gap-3">
              <Field label={t('edit_weight')}>
                <Input
                  aria-label={t('edit_weight')}
                  autoComplete="off"
                  name="weight"
                  onChange={(e) => setWeight(Number.parseInt(e.target.value, 10) || 0)}
                  type="number"
                  value={weight}
                />
              </Field>
              <Field label={t('edit_hidden')}>
                {/* biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element */}
                <label className="flex cursor-pointer items-center gap-2 pt-1">
                  <Checkbox checked={hidden} onCheckedChange={(checked) => setHidden(!!checked)} />
                  <span className="text-sm">{t('edit_hide_from_status')}</span>
                </label>
              </Field>
            </div>
            <Field label={t('edit_group')}>
              <Select
                items={[
                  { value: '__none__', label: t('edit_no_group') },
                  ...(groups?.map((g) => ({ value: g.id, label: g.name })) ?? [])
                ]}
                onValueChange={(v) => setGroupId(v === '__none__' || v === null ? '' : v)}
                value={groupId || '__none__'}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="__none__">{t('edit_no_group')}</SelectItem>
                  {groups?.map((g) => (
                    <SelectItem key={g.id} value={g.id}>
                      {g.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <Field label={t('edit_remark')}>
              <Input
                aria-label={t('edit_remark')}
                name="remark"
                onChange={(e) => setRemark(e.target.value)}
                placeholder={t('edit_remark_placeholder')}
                type="text"
                value={remark}
              />
            </Field>
            <Field label={t('edit_public_remark')}>
              <Input
                aria-label={t('edit_public_remark')}
                name="public_remark"
                onChange={(e) => setPublicRemark(e.target.value)}
                placeholder={t('edit_public_remark_placeholder')}
                type="text"
                value={publicRemark}
              />
            </Field>
            <Field label={t('tags_label')}>
              <Input
                aria-label={t('tags_label')}
                name="tags"
                onChange={(e) => {
                  setTagsInput(e.target.value)
                  setTagsDirty(true)
                }}
                placeholder={t('tags_placeholder')}
                type="text"
                value={tagsInput}
              />
              <p className="mt-1 text-[11px] text-muted-foreground">{t('tags_hint')}</p>
            </Field>
          </fieldset>

          {/* Billing */}
          <fieldset className="space-y-3">
            <legend className="mb-1 font-medium text-muted-foreground text-xs uppercase tracking-wider">
              {t('edit_billing')}
            </legend>
            <div className="grid grid-cols-3 gap-3">
              <Field label={t('edit_price')}>
                <Input
                  aria-label={t('edit_price')}
                  autoComplete="off"
                  min="0"
                  name="price"
                  onChange={(e) => setPrice(e.target.value)}
                  placeholder="0.00"
                  step="0.01"
                  type="number"
                  value={price}
                />
              </Field>
              <Field label={t('edit_currency')}>
                <Select onValueChange={(v) => v !== null && setCurrency(v)} value={currency}>
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="USD">USD</SelectItem>
                    <SelectItem value="EUR">EUR</SelectItem>
                    <SelectItem value="CNY">CNY</SelectItem>
                    <SelectItem value="JPY">JPY</SelectItem>
                    <SelectItem value="GBP">GBP</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
              <Field label={t('edit_billing_cycle')}>
                <Select
                  items={{
                    __none__: t('edit_cycle_none'),
                    monthly: t('edit_cycle_monthly'),
                    quarterly: t('edit_cycle_quarterly'),
                    yearly: t('edit_cycle_yearly')
                  }}
                  onValueChange={(v) => setBillingCycle(v === '__none__' || v === null ? '' : v)}
                  value={billingCycle || '__none__'}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="__none__">{t('edit_cycle_none')}</SelectItem>
                    <SelectItem value="monthly">{t('edit_cycle_monthly')}</SelectItem>
                    <SelectItem value="quarterly">{t('edit_cycle_quarterly')}</SelectItem>
                    <SelectItem value="yearly">{t('edit_cycle_yearly')}</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
            </div>
            <Field label={t('edit_expiration')}>
              <DatePickerField ariaLabel={t('edit_expiration')} onChange={setExpiredAt} value={expiredAt} />
            </Field>
            <div className="grid grid-cols-2 gap-3">
              <Field label={t('edit_traffic_limit')}>
                <Input
                  aria-label={t('edit_traffic_limit')}
                  autoComplete="off"
                  min="0"
                  name="traffic_limit"
                  onChange={(e) => setTrafficLimit(e.target.value)}
                  placeholder={t('edit_unlimited')}
                  step="0.1"
                  type="number"
                  value={trafficLimit}
                />
              </Field>
              <Field label={t('edit_limit_type')}>
                <Select
                  items={{
                    sum: t('edit_limit_total'),
                    up: t('edit_limit_upload'),
                    down: t('edit_limit_download')
                  }}
                  onValueChange={(v) => v !== null && setTrafficLimitType(v)}
                  value={trafficLimitType}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="sum">{t('edit_limit_total')}</SelectItem>
                    <SelectItem value="up">{t('edit_limit_upload')}</SelectItem>
                    <SelectItem value="down">{t('edit_limit_download')}</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
            </div>
            <Field label={t('edit_billing_start_day', { defaultValue: 'Billing Start Day' })}>
              <Input
                aria-label={t('edit_billing_start_day', { defaultValue: 'Billing Start Day' })}
                autoComplete="off"
                max="28"
                min="1"
                name="billing_start_day"
                onChange={(e) => setBillingStartDay(e.target.value)}
                placeholder={t('edit_billing_start_day_placeholder', {
                  defaultValue: 'Leave empty for natural month (1st)'
                })}
                type="number"
                value={billingStartDay}
              />
            </Field>
          </fieldset>

          {mutation.error && (
            <div className="rounded-md bg-destructive/10 px-3 py-2 text-destructive text-sm">
              {mutation.error.message || t('edit_failed')}
            </div>
          )}

          <div className="flex justify-end gap-2 pt-2">
            <Button onClick={onClose} type="button" variant="outline">
              {t('common:cancel')}
            </Button>
            <Button disabled={mutation.isPending || tagsMutation.isPending} type="submit">
              {mutation.isPending || tagsMutation.isPending ? t('common:saving') : t('common:save')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function Field({ label, children }: { children: React.ReactNode; label: string }) {
  return (
    <div className="space-y-1">
      {/* biome-ignore lint/a11y/noLabelWithoutControl: label wraps child input via adjacent sibling pattern */}
      <label className="font-medium text-sm">{label}</label>
      {children}
    </div>
  )
}

interface DatePickerFieldProps {
  ariaLabel: string
  onChange: (value: string) => void
  value: string
}

function DatePickerField({ ariaLabel, onChange, value }: DatePickerFieldProps) {
  const { t } = useTranslation('servers')
  const selected = value ? new Date(`${value}T00:00:00`) : undefined
  return (
    <div>
      <Popover>
        <PopoverTrigger
          render={
            <Button
              aria-label={ariaLabel}
              className="w-full justify-start font-normal"
              type="button"
              variant="outline"
            />
          }
        >
          <CalendarIcon className="size-4 text-muted-foreground" />
          <span className={value ? '' : 'text-muted-foreground'}>
            {value || t('edit_expiration_placeholder', { defaultValue: 'YYYY-MM-DD' })}
          </span>
        </PopoverTrigger>
        <PopoverContent align="start" className="w-auto p-0">
          <Calendar
            captionLayout="dropdown"
            mode="single"
            onSelect={(date) => onChange(date ? formatIsoDate(date) : '')}
            selected={selected}
          />
        </PopoverContent>
      </Popover>
    </div>
  )
}
