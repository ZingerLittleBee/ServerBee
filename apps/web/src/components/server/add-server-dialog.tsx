import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CalendarIcon, Copy, Plus } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Calendar } from '@/components/ui/calendar'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { reconcileServersFromRest, type ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import type { CreateServerRequest, CreateServerResponse, ServerGroup, ServerResponse } from '@/lib/api-schema'
import { CAP_DEFAULT, CAPABILITIES, hasCap } from '@/lib/capabilities'
import { cn } from '@/lib/utils'

const DEFAULT_CAP_KEYS = CAPABILITIES.flatMap((c) => (hasCap(CAP_DEFAULT, c.bit) ? [c.key] : []))
const ALL_CAP_KEYS = CAPABILITIES.map((c) => c.key)

const TAG_SPLIT_RE = /[\s,]+/
const TAG_VALID_RE = /^[A-Za-z0-9_.-]+$/

function formatIsoDate(date: Date): string {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}

function parseFloatOrNaN(raw: string): number {
  if (!raw) {
    return Number.NaN
  }
  return Number.parseFloat(raw)
}

function parseIntOrNaN(raw: string): number {
  if (!raw) {
    return Number.NaN
  }
  return Number.parseInt(raw, 10)
}

function nullIfBlank(raw: string): string | undefined {
  const trimmed = raw.trim()
  return trimmed || undefined
}

function numberOrUndefined(value: number): number | undefined {
  return Number.isNaN(value) ? undefined : value
}

function parseTagsInput(raw: string): { tags: string[]; error: string | null } {
  const parts = raw.split(TAG_SPLIT_RE).flatMap((t) => {
    const tag = t.trim()
    return tag ? [tag] : []
  })
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

interface CapGroupProps {
  caps: readonly (typeof CAPABILITIES)[number][]
  onToggle: (key: string) => void
  selected: Set<string>
  t: (key: string) => string
  title: string
  tone: 'high' | 'standard'
}

function CapGroup({ caps, onToggle, selected, t, title, tone }: CapGroupProps) {
  return (
    <div>
      <p
        className={cn(
          'mb-1.5 font-medium text-[11px] uppercase tracking-wide',
          tone === 'high' ? 'text-amber-600 dark:text-amber-500' : 'text-muted-foreground'
        )}
      >
        {title}
      </p>
      <div className="grid grid-cols-1 gap-1.5 sm:grid-cols-2">
        {caps.map((cap) => {
          const id = `add-server-cap-${cap.key}`
          return (
            <label className="flex cursor-pointer items-center gap-2 text-sm" htmlFor={id} key={cap.key}>
              <Checkbox checked={selected.has(cap.key)} id={id} onCheckedChange={() => onToggle(cap.key)} />
              <span className="truncate">{t(cap.labelKey)}</span>
            </label>
          )
        })}
      </div>
    </div>
  )
}

function Field({ label, children, htmlFor }: { children: React.ReactNode; htmlFor?: string; label: string }) {
  return (
    <div className="space-y-1">
      <label className="font-medium text-sm" htmlFor={htmlFor}>
        {label}
      </label>
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

export function AddServerDialog({ open, onClose }: { onClose: () => void; open: boolean }) {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()

  const [name, setName] = useState('')
  const [groupId, setGroupId] = useState('')
  const [tagsInput, setTagsInput] = useState('')
  const [remark, setRemark] = useState('')
  const [publicRemark, setPublicRemark] = useState('')
  const [price, setPrice] = useState('')
  const [currency, setCurrency] = useState('USD')
  const [billingCycle, setBillingCycle] = useState('')
  const [billingStartDay, setBillingStartDay] = useState('')
  const [expiredAt, setExpiredAt] = useState('')
  const [trafficLimit, setTrafficLimit] = useState('')
  const [trafficLimitType, setTrafficLimitType] = useState('sum')
  const [selectedCaps, setSelectedCaps] = useState<Set<string>>(() => new Set(DEFAULT_CAP_KEYS))
  const [issued, setIssued] = useState<CreateServerResponse | null>(null)

  const { data: groups } = useQuery<ServerGroup[]>({
    queryKey: ['server-groups'],
    queryFn: () => api.get<ServerGroup[]>('/api/server-groups'),
    staleTime: 60_000,
    enabled: open
  })

  const mutation = useMutation({
    mutationFn: (body: CreateServerRequest) => api.post<CreateServerResponse>('/api/servers', body),
    onSuccess: async (data) => {
      setIssued(data)
      // ['servers'] is a WS-fed cache (queryFn: () => []); invalidating it
      // would wipe the list. Refresh membership from REST and reconcile —
      // existing rows keep their runtime metrics, the brand-new row is
      // inserted as a stub until the next WS push fills it in.
      try {
        const fresh = await api.get<ServerResponse[]>('/api/servers')
        queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
          reconcileServersFromRest(prev, fresh as unknown as Array<Partial<ServerMetrics> & { id: string }>)
        )
      } catch {
        // Best-effort: the new row will surface on the next WS full_sync.
      }
    },
    onError: (err: unknown) => {
      toast.error(err instanceof Error ? err.message : t('add_server.generate_failed'))
    }
  })

  const origin = typeof window !== 'undefined' ? window.location.origin : ''
  // Emit --caps only when the selection differs from the default set; an
  // omitted flag means "use install.sh's built-in defaults", which keeps the
  // copy/paste command short for the common case.
  const orderedCapSelection = ALL_CAP_KEYS.filter((k) => selectedCaps.has(k))
  const capsIsDefault =
    orderedCapSelection.length === DEFAULT_CAP_KEYS.length && DEFAULT_CAP_KEYS.every((k) => selectedCaps.has(k))
  const capsArg = (() => {
    if (capsIsDefault) {
      return ''
    }
    if (orderedCapSelection.length === 0) {
      return " --caps ''"
    }
    return ` --caps ${orderedCapSelection.join(',')}`
  })()
  const installCommand = issued
    ? `curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- agent --server-url '${origin}' --enrollment-code '${issued.enrollment.code}'${capsArg}`
    : ''

  const toggleCap = (key: string) => {
    setSelectedCaps((prev) => {
      const next = new Set(prev)
      if (next.has(key)) {
        next.delete(key)
      } else {
        next.add(key)
      }
      return next
    })
  }
  const resetCapsToDefault = () => setSelectedCaps(new Set(DEFAULT_CAP_KEYS))
  const selectAllCaps = () => setSelectedCaps(new Set(ALL_CAP_KEYS))
  const selectNoCaps = () => setSelectedCaps(new Set())

  const highRiskCaps = CAPABILITIES.filter((c) => c.risk === 'high')
  const standardCaps = CAPABILITIES.filter((c) => c.risk !== 'high')

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
    setName('')
    setGroupId('')
    setTagsInput('')
    setRemark('')
    setPublicRemark('')
    setPrice('')
    setCurrency('USD')
    setBillingCycle('')
    setBillingStartDay('')
    setExpiredAt('')
    setTrafficLimit('')
    setTrafficLimitType('sum')
    setSelectedCaps(new Set(DEFAULT_CAP_KEYS))
  }

  const handleClose = () => {
    reset()
    onClose()
  }

  const buildBody = (trimmedName: string, tags: string[]): CreateServerRequest => {
    const trafficLimitValue = numberOrUndefined(Math.round(parseFloatOrNaN(trafficLimit) * 1024 ** 3))
    const optionalFields: Partial<CreateServerRequest> = {
      group_id: groupId || undefined,
      tags: tags.length > 0 ? tags : undefined,
      remark: nullIfBlank(remark),
      public_remark: nullIfBlank(publicRemark),
      price: numberOrUndefined(parseFloatOrNaN(price)),
      currency: currency || undefined,
      billing_cycle: billingCycle || undefined,
      billing_start_day: numberOrUndefined(parseIntOrNaN(billingStartDay)),
      expired_at: expiredAt ? `${expiredAt}T00:00:00Z` : undefined,
      traffic_limit: trafficLimitValue,
      traffic_limit_type: trafficLimitValue === undefined ? undefined : trafficLimitType,
      caps: capsIsDefault ? undefined : orderedCapSelection
    }
    const body: CreateServerRequest = { name: trimmedName }
    for (const [k, v] of Object.entries(optionalFields)) {
      if (v !== undefined) {
        ;(body as Record<string, unknown>)[k] = v
      }
    }
    return body
  }

  const handleSubmit = (e?: FormEvent) => {
    e?.preventDefault()
    const trimmedName = name.trim()
    if (!trimmedName) {
      return
    }
    const parsed = parseTagsInput(tagsInput)
    if (parsed.error) {
      toast.error(t(parsed.error))
      return
    }
    mutation.mutate(buildBody(trimmedName, parsed.tags))
  }

  const submitDisabled = mutation.isPending || !name.trim()

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

        {issued ? (
          <>
            <DialogBody className="space-y-5">
              <p className="text-muted-foreground text-sm">{t('add_server.description')}</p>

              <div className="space-y-4 rounded-md border border-amber-500/40 bg-amber-500/5 p-4">
                <p className="text-amber-600 text-sm dark:text-amber-500">{t('add_server.shown_once_warning')}</p>

                <div>
                  <p className="mb-1 font-medium text-muted-foreground text-xs">{t('add_server.code_label')}</p>
                  <div className="flex min-w-0 items-center gap-2">
                    <code className="min-w-0 flex-1 truncate rounded-md border bg-muted/50 px-3 py-2 font-mono text-sm">
                      {issued.enrollment.code}
                    </code>
                    <Button
                      aria-label={t('add_server.copy')}
                      onClick={() => copy(issued.enrollment.code)}
                      size="icon"
                      type="button"
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
                      type="button"
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
            </DialogBody>

            <DialogFooter>
              <Button onClick={reset} type="button" variant="outline">
                {t('add_server.another')}
              </Button>
              <Button onClick={handleClose} type="button">
                {t('add_server.done')}
              </Button>
            </DialogFooter>
          </>
        ) : (
          <form className="flex min-h-0 flex-1 flex-col gap-4" onSubmit={handleSubmit}>
            <DialogBody className="space-y-4">
              <p className="text-muted-foreground text-sm">{t('add_server.description')}</p>

              {/* Basic */}
              <fieldset className="space-y-3">
                <legend className="mb-1 font-medium text-muted-foreground text-xs uppercase tracking-wider">
                  {t('edit_basic')}
                </legend>
                <Field htmlFor="add-server-name" label={t('add_server.name_label')}>
                  <Input
                    aria-label={t('add_server.name_label')}
                    autoComplete="off"
                    id="add-server-name"
                    name="name"
                    onChange={(e) => setName(e.target.value)}
                    placeholder={t('add_server.name_placeholder')}
                    required
                    type="text"
                    value={name}
                  />
                </Field>
                <Field label={t('add_server.group_label')}>
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
                <Field label={t('add_server.tags_label')}>
                  <Input
                    aria-label={t('add_server.tags_label')}
                    autoComplete="off"
                    name="tags"
                    onChange={(e) => setTagsInput(e.target.value)}
                    placeholder={t('tags_placeholder')}
                    type="text"
                    value={tagsInput}
                  />
                  <p className="mt-1 text-[11px] text-muted-foreground">{t('tags_hint')}</p>
                </Field>
                <Field label={t('add_server.remark_label')}>
                  <Input
                    aria-label={t('add_server.remark_label')}
                    autoComplete="off"
                    name="remark"
                    onChange={(e) => setRemark(e.target.value)}
                    placeholder={t('edit_remark_placeholder')}
                    type="text"
                    value={remark}
                  />
                </Field>
                <Field label={t('add_server.public_remark_label')}>
                  <Input
                    aria-label={t('add_server.public_remark_label')}
                    autoComplete="off"
                    name="public_remark"
                    onChange={(e) => setPublicRemark(e.target.value)}
                    placeholder={t('edit_public_remark_placeholder')}
                    type="text"
                    value={publicRemark}
                  />
                </Field>
              </fieldset>

              {/* Billing */}
              <fieldset className="space-y-3">
                <legend className="mb-1 font-medium text-muted-foreground text-xs uppercase tracking-wider">
                  {t('add_server.billing_section')}
                </legend>
                <div className="grid gap-3 sm:grid-cols-3">
                  <Field label={t('add_server.price_label')}>
                    <Input
                      aria-label={t('add_server.price_label')}
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
                  <Field label={t('add_server.currency_label')}>
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
                  <Field label={t('add_server.billing_cycle_label')}>
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
                <Field label={t('add_server.expired_at_label')}>
                  <DatePickerField
                    ariaLabel={t('add_server.expired_at_label')}
                    onChange={setExpiredAt}
                    value={expiredAt}
                  />
                </Field>
                <div className="grid gap-3 sm:grid-cols-2">
                  <Field label={t('add_server.traffic_limit_label')}>
                    <Input
                      aria-label={t('add_server.traffic_limit_label')}
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
                  <Field label={t('add_server.traffic_limit_type_label')}>
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
                <Field label={t('add_server.billing_start_day_label')}>
                  <Input
                    aria-label={t('add_server.billing_start_day_label')}
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

              {/* Capabilities */}
              <fieldset className="space-y-2">
                <legend className="mb-1 flex w-full items-center justify-between gap-2">
                  <span className="font-medium text-muted-foreground text-xs uppercase tracking-wider">
                    {t('add_server.caps_label')}
                  </span>
                  <span className="flex gap-2 text-xs">
                    <button
                      className="text-muted-foreground hover:text-foreground"
                      onClick={resetCapsToDefault}
                      type="button"
                    >
                      {t('add_server.caps_reset')}
                    </button>
                    <span className="text-muted-foreground/50">·</span>
                    <button
                      className="text-muted-foreground hover:text-foreground"
                      onClick={selectAllCaps}
                      type="button"
                    >
                      {t('add_server.caps_select_all')}
                    </button>
                    <span className="text-muted-foreground/50">·</span>
                    <button
                      className="text-muted-foreground hover:text-foreground"
                      onClick={selectNoCaps}
                      type="button"
                    >
                      {t('add_server.caps_select_none')}
                    </button>
                  </span>
                </legend>
                <p className="text-muted-foreground text-xs">{t('add_server.caps_hint')}</p>
                <div className="mt-2 space-y-3 rounded-md border bg-muted/30 p-3">
                  <CapGroup
                    caps={standardCaps}
                    onToggle={toggleCap}
                    selected={selectedCaps}
                    t={t}
                    title={t('add_server.caps_low_risk')}
                    tone="standard"
                  />
                  <CapGroup
                    caps={highRiskCaps}
                    onToggle={toggleCap}
                    selected={selectedCaps}
                    t={t}
                    title={t('add_server.caps_high_risk')}
                    tone="high"
                  />
                </div>
              </fieldset>

              <p className="text-muted-foreground text-xs">{t('add_server.ttl_tip')}</p>
            </DialogBody>

            <DialogFooter>
              <Button onClick={handleClose} type="button" variant="outline">
                {t('common:cancel')}
              </Button>
              <Button
                className={cn(mutation.isPending && 'pointer-events-none opacity-70')}
                disabled={submitDisabled}
                type="submit"
              >
                <Plus className="size-4" />
                {mutation.isPending ? t('add_server.generating') : t('add_server.generate')}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  )
}
