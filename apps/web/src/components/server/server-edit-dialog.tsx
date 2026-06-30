import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CalendarIcon, Check, ChevronsUpDown } from 'lucide-react'
import { type FormEvent, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Calendar } from '@/components/ui/calendar'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator
} from '@/components/ui/command'
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useServerTags, useUpdateServerTags } from '@/hooks/use-server-tags'
import { applyServerEdit, type ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import type { ServerGroup, ServerResponse, UpdateServerInput } from '@/lib/api-schema'
import { buildCountryOptions, type CountryOption } from '@/lib/country-codes'
import { cn, countryCodeToFlag } from '@/lib/utils'

const TAG_SPLIT_RE = /[\s,]+/
const TAG_VALID_RE = /^[A-Za-z0-9_.-]+$/

function formatIsoDate(date: Date): string {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
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

interface ServerEditDialogProps {
  onClose: () => void
  open: boolean
  server: ServerResponse
}

export function ServerEditDialog({ server, open, onClose }: ServerEditDialogProps) {
  return (
    <Dialog
      onOpenChange={(isOpen) => {
        if (!isOpen) {
          onClose()
        }
      }}
      open={open}
    >
      {open && <ServerEditDialogContent key={server.id} onClose={onClose} server={server} />}
    </Dialog>
  )
}

function ServerEditDialogContent({ server, onClose }: { onClose: () => void; server: ServerResponse }) {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()
  const [name, setName] = useState(() => server.name)
  const [weight, setWeight] = useState(() => server.weight)
  const [hidden, setHidden] = useState(() => server.hidden)
  const [groupId, setGroupId] = useState(() => server.group_id ?? '')
  const [remark, setRemark] = useState(() => server.remark ?? '')
  const [publicRemark, setPublicRemark] = useState(() => server.public_remark ?? '')
  // The country override field is empty unless the server already has a manual
  // override; otherwise we leave it blank and surface the auto-detected value as
  // a hint, so saving an untouched form never accidentally pins GeoIP.
  const [initialCountryCode] = useState(() => (server.geo_manual ? (server.country_code ?? '') : ''))
  const [countryCode, setCountryCode] = useState(initialCountryCode)
  const [price, setPrice] = useState(() => server.price?.toString() ?? '')
  const [billingCycle, setBillingCycle] = useState(() => server.billing_cycle ?? '')
  const [currency, setCurrency] = useState(() => server.currency ?? 'USD')
  const [expiredAt, setExpiredAt] = useState(() => server.expired_at?.slice(0, 10) ?? '')
  const [trafficLimit, setTrafficLimit] = useState(() =>
    server.traffic_limit ? (server.traffic_limit / 1024 ** 3).toString() : ''
  )
  const [trafficLimitType, setTrafficLimitType] = useState(() => server.traffic_limit_type ?? 'sum')
  const [billingStartDay, setBillingStartDay] = useState(() => server.billing_start_day?.toString() ?? '')
  const [tagsDraft, setTagsDraft] = useState<{ dirty: boolean; value: string }>({ dirty: false, value: '' })

  const { data: groups } = useQuery<ServerGroup[]>({
    queryKey: ['server-groups'],
    queryFn: () => api.get<ServerGroup[]>('/api/server-groups'),
    staleTime: 60_000,
    enabled: true
  })

  const { data: initialTags } = useServerTags(server.id, true)
  const tagsMutation = useUpdateServerTags(server.id)
  const tagsInput = tagsDraft.dirty ? tagsDraft.value : (initialTags?.join(', ') ?? '')

  const mutation = useMutation({
    mutationFn: (payload: UpdateServerInput) => api.put<ServerResponse>(`/api/servers/${server.id}`, payload),
    onSuccess: (data) => {
      queryClient.setQueryData(['servers', server.id], data)
      queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
        prev
          ? applyServerEdit(prev, server.id, {
              name: data.name,
              group_id: data.group_id ?? null,
              country_code: data.country_code ?? null
            })
          : prev
      )
    }
  })

  const buildPayload = (): UpdateServerInput => {
    const payload: UpdateServerInput = {
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
      billing_start_day: billingStartDay ? Number.parseInt(billingStartDay, 10) : null,
      ...countryCodePatch(countryCode, initialCountryCode)
    }
    return payload
  }

  const saveTags = async (tags: string[]): Promise<boolean> => {
    try {
      await tagsMutation.mutateAsync(tags)
      return true
    } catch (err) {
      if (initialTags) {
        setTagsDraft({ dirty: false, value: '' })
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
    if (tagsDraft.dirty && !(await saveTags(parsed.tags))) {
      return
    }
    toast.success(t('edit_success', { defaultValue: 'Server updated successfully' }))
    onClose()
  }

  return (
    <DialogContent className="sm:max-w-lg">
      <DialogHeader>
        <DialogTitle>{t('edit_title')}</DialogTitle>
      </DialogHeader>

      <form className="flex min-h-0 flex-1 flex-col gap-4" onSubmit={handleSubmit}>
        <DialogBody className="space-y-4">
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
            <div className="grid gap-3 sm:grid-cols-2">
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
            <CountryOverrideField onChange={setCountryCode} server={server} value={countryCode} />
            <Field label={t('tags_label')}>
              <Input
                aria-label={t('tags_label')}
                name="tags"
                onChange={(e) => {
                  setTagsDraft({ dirty: true, value: e.target.value })
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
            <div className="grid gap-3 sm:grid-cols-3">
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
            <div className="grid gap-3 sm:grid-cols-2">
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
        </DialogBody>

        <DialogFooter>
          <Button onClick={onClose} type="button" variant="outline">
            {t('common:cancel')}
          </Button>
          <Button disabled={mutation.isPending || tagsMutation.isPending} type="submit">
            {mutation.isPending || tagsMutation.isPending ? t('common:saving') : t('common:save')}
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
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

// Only emit country_code when the override field actually changed, so saving an
// untouched form never flips an auto-detected server to a manual one. An emptied
// override sends null, which clears the override and resumes GeoIP detection.
function countryCodePatch(current: string, initial: string): { country_code?: string | null } {
  const normalized = current.trim().toUpperCase()
  if (normalized === initial.toUpperCase()) {
    return {}
  }
  return { country_code: normalized || null }
}

function CountryCommandItem({
  option,
  selected,
  onSelect
}: {
  onSelect: (code: string) => void
  option: CountryOption
  selected: boolean
}) {
  return (
    <CommandItem keywords={[option.name]} onSelect={() => onSelect(option.code)} value={option.code}>
      <Check className={cn('size-4 shrink-0', selected ? 'opacity-100' : 'opacity-0')} />
      <span aria-hidden="true" className="text-base leading-none">
        {option.flag}
      </span>
      <span className="flex-1 truncate">{option.name}</span>
      <span className="text-muted-foreground text-xs tabular-nums" data-slot="command-shortcut">
        {option.code}
      </span>
    </CommandItem>
  )
}

function CountryOverrideField({
  value,
  onChange,
  server
}: {
  onChange: (value: string) => void
  server: ServerResponse
  value: string
}) {
  const { t, i18n } = useTranslation('servers')
  const [open, setOpen] = useState(false)
  const { common, rest } = useMemo(() => buildCountryOptions(i18n.language), [i18n.language])
  const current = value.toUpperCase()
  const selected = common.find((option) => option.code === current) ?? rest.find((option) => option.code === current)
  const autoLabel = t('edit_country_auto_option')

  function handleSelect(code: string) {
    onChange(code)
    setOpen(false)
  }

  return (
    <Field label={t('edit_country')}>
      <Popover onOpenChange={setOpen} open={open}>
        <PopoverTrigger
          render={
            <Button className="w-full justify-between font-normal" type="button" variant="outline">
              <span className="flex min-w-0 items-center gap-2">
                <span aria-hidden="true" className="text-base leading-none">
                  {countryCodeToFlag(value) || '🏳️'}
                </span>
                <span className="truncate">{selected?.name ?? (current || autoLabel)}</span>
              </span>
              <ChevronsUpDown className="size-4 shrink-0 opacity-50" />
            </Button>
          }
        />
        <PopoverContent align="start" className="w-(--anchor-width) p-0">
          <Command>
            <CommandInput placeholder={t('edit_country_search')} />
            <CommandList>
              <CommandEmpty>{t('edit_country_empty')}</CommandEmpty>
              <CommandGroup>
                <CommandItem keywords={[autoLabel]} onSelect={() => handleSelect('')} value="__auto__">
                  <Check className={cn('size-4 shrink-0', current ? 'opacity-0' : 'opacity-100')} />
                  <span aria-hidden="true" className="text-base leading-none">
                    🏳️
                  </span>
                  <span className="flex-1 truncate">{autoLabel}</span>
                  <span className="sr-only" data-slot="command-shortcut" />
                </CommandItem>
              </CommandGroup>
              <CommandSeparator />
              <CommandGroup heading={t('edit_country_common')}>
                {common.map((option) => (
                  <CountryCommandItem
                    key={option.code}
                    onSelect={handleSelect}
                    option={option}
                    selected={current === option.code}
                  />
                ))}
              </CommandGroup>
              <CommandSeparator />
              <CommandGroup heading={t('edit_country_all')}>
                {rest.map((option) => (
                  <CountryCommandItem
                    key={option.code}
                    onSelect={handleSelect}
                    option={option}
                    selected={current === option.code}
                  />
                ))}
              </CommandGroup>
            </CommandList>
          </Command>
        </PopoverContent>
      </Popover>
      <p className="mt-1 text-[11px] text-muted-foreground">
        {server.geo_manual
          ? t('edit_country_hint_manual')
          : t('edit_country_hint_auto', { value: server.country_code || '—' })}
      </p>
    </Field>
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
