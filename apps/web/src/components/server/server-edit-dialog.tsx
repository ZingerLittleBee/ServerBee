import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { TFunction } from 'i18next'
import { Check, ChevronsUpDown } from 'lucide-react'
import { type FormEvent, useId, useMemo, useReducer, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { RenewalDeadlineInfo } from '@/components/server/renewal-deadline-info'
import { Button } from '@/components/ui/button'
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
import { Switch } from '@/components/ui/switch'
import { useServerTags, useUpdateServerTags } from '@/hooks/use-server-tags'
import { api } from '@/lib/api-client'
import type { ServerGroup, ServerResponse, UpdateServerInput } from '@/lib/api-schema'
import { buildCountryOptions, type CountryOption } from '@/lib/country-codes'
import { projectServerCatalog } from '@/lib/server-catalog'
import { invalidateServerCosts } from '@/lib/server-cost-cache'
import { cn, countryCodeToFlag } from '@/lib/utils'

const TAG_SPLIT_RE = /[\s,]+/
const TAG_VALID_RE = /^[A-Za-z0-9_.-]+$/

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

interface ServerEditState {
  automaticRenewal: boolean
  billingCycle: string
  billingStartDay: string
  billingTimezone: string
  countryCode: string
  currency: string
  expiredAt: string
  groupId: string
  hidden: boolean
  name: string
  price: string
  publicRemark: string
  remark: string
  tagsDraft: { dirty: boolean; value: string }
  trafficLimit: string
  trafficLimitType: string
  weight: number
}

interface ServerEditAction {
  type: 'patch'
  value: Partial<ServerEditState>
}

function serverEditStateFromServer(server: ServerResponse): ServerEditState {
  return {
    automaticRenewal: server.renewal?.enabled ?? false,
    billingCycle: server.billing_cycle ?? '',
    billingStartDay: server.billing_start_day?.toString() ?? '',
    billingTimezone: server.renewal?.billing_timezone ?? 'UTC',
    countryCode: server.geo_manual ? (server.country_code ?? '') : '',
    currency: server.currency ?? 'USD',
    expiredAt: server.renewal ? (server.renewal.expiry_date ?? '') : (server.expired_at?.slice(0, 10) ?? ''),
    groupId: server.group_id ?? '',
    hidden: server.hidden,
    name: server.name,
    price: server.price?.toString() ?? '',
    publicRemark: server.public_remark ?? '',
    remark: server.remark ?? '',
    tagsDraft: { dirty: false, value: '' },
    trafficLimit: server.traffic_limit ? (server.traffic_limit / 1024 ** 3).toString() : '',
    trafficLimitType: server.traffic_limit_type ?? 'sum',
    weight: server.weight
  }
}

function serverEditReducer(state: ServerEditState, action: ServerEditAction): ServerEditState {
  switch (action.type) {
    case 'patch':
      return { ...state, ...action.value }
    default:
      return state
  }
}

function ServerEditBasicFields({
  dispatch,
  groups,
  server,
  state,
  tagsInput,
  t
}: {
  dispatch: (action: ServerEditAction) => void
  groups: ServerGroup[] | undefined
  server: ServerResponse
  state: ServerEditState
  tagsInput: string
  t: TFunction
}) {
  return (
    <fieldset className="space-y-3">
      <legend className="mb-1 font-medium text-muted-foreground text-xs uppercase tracking-wider">
        {t('edit_basic')}
      </legend>
      <Field label={t('edit_name')}>
        <Input
          aria-label={t('edit_name')}
          name="name"
          onChange={(e) => dispatch({ type: 'patch', value: { name: e.target.value } })}
          required
          type="text"
          value={state.name}
        />
      </Field>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label={t('edit_weight')}>
          <Input
            aria-label={t('edit_weight')}
            autoComplete="off"
            name="weight"
            onChange={(e) => dispatch({ type: 'patch', value: { weight: Number.parseInt(e.target.value, 10) || 0 } })}
            type="number"
            value={state.weight}
          />
        </Field>
        <Field label={t('edit_hidden')}>
          {/* biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element */}
          <label className="flex cursor-pointer items-center gap-2 pt-1">
            <Checkbox
              checked={state.hidden}
              onCheckedChange={(checked) => dispatch({ type: 'patch', value: { hidden: !!checked } })}
            />
            <span className="text-sm">{t('edit_hide_from_status')}</span>
          </label>
        </Field>
      </div>
      <Field label={t('edit_group')}>
        <Select
          items={[
            { value: '__none__', label: t('edit_no_group') },
            ...(groups?.map((group) => ({ value: group.id, label: group.name })) ?? [])
          ]}
          onValueChange={(value) =>
            dispatch({ type: 'patch', value: { groupId: value === '__none__' || value === null ? '' : value } })
          }
          value={state.groupId || '__none__'}
        >
          <SelectTrigger className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__none__">{t('edit_no_group')}</SelectItem>
            {groups?.map((group) => (
              <SelectItem key={group.id} value={group.id}>
                {group.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      <Field label={t('edit_remark')}>
        <Input
          aria-label={t('edit_remark')}
          name="remark"
          onChange={(e) => dispatch({ type: 'patch', value: { remark: e.target.value } })}
          placeholder={t('edit_remark_placeholder')}
          type="text"
          value={state.remark}
        />
      </Field>
      <Field label={t('edit_public_remark')}>
        <Input
          aria-label={t('edit_public_remark')}
          name="public_remark"
          onChange={(e) => dispatch({ type: 'patch', value: { publicRemark: e.target.value } })}
          placeholder={t('edit_public_remark_placeholder')}
          type="text"
          value={state.publicRemark}
        />
      </Field>
      <CountryOverrideField
        onChange={(countryCode) => dispatch({ type: 'patch', value: { countryCode } })}
        server={server}
        value={state.countryCode}
      />
      <Field label={t('tags_label')}>
        <Input
          aria-label={t('tags_label')}
          name="tags"
          onChange={(e) => dispatch({ type: 'patch', value: { tagsDraft: { dirty: true, value: e.target.value } } })}
          placeholder={t('tags_placeholder')}
          type="text"
          value={tagsInput}
        />
        <p className="mt-1 text-[11px] text-muted-foreground">{t('tags_hint')}</p>
      </Field>
    </fieldset>
  )
}

function ServerEditBillingFields({
  dispatch,
  server,
  state,
  t
}: {
  dispatch: (action: ServerEditAction) => void
  server: ServerResponse
  state: ServerEditState
  t: TFunction
}) {
  const timezoneListId = useId()
  const prerequisitesId = useId()
  const hasPrerequisites = hasAutomaticRenewalPrerequisites(state)
  const timezones = useMemo(
    () => ['UTC', ...(typeof Intl.supportedValuesOf === 'function' ? Intl.supportedValuesOf('timeZone') : [])],
    []
  )
  return (
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
            onChange={(e) => dispatch({ type: 'patch', value: { price: e.target.value } })}
            placeholder="0.00"
            step="0.01"
            type="number"
            value={state.price}
          />
        </Field>
        <Field label={t('edit_currency')}>
          <Select
            onValueChange={(value) => value !== null && dispatch({ type: 'patch', value: { currency: value } })}
            value={state.currency}
          >
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
            onValueChange={(value) =>
              dispatch({
                type: 'patch',
                value: { billingCycle: value === '__none__' || value === null ? '' : value }
              })
            }
            value={state.billingCycle || '__none__'}
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
        <ExpiryDateField
          ariaLabel={t('edit_expiration')}
          onChange={(expiredAt) => dispatch({ type: 'patch', value: { expiredAt } })}
          value={state.expiredAt}
        />
      </Field>
      <Field label={t('edit_billing_timezone')}>
        <Input
          aria-label={t('edit_billing_timezone')}
          autoComplete="off"
          list={timezoneListId}
          name="billing_timezone"
          onChange={(e) => dispatch({ type: 'patch', value: { billingTimezone: e.target.value } })}
          placeholder="America/New_York"
          type="text"
          value={state.billingTimezone}
        />
        <datalist id={timezoneListId}>
          {timezones.map((timezone) => (
            <option key={timezone} value={timezone} />
          ))}
        </datalist>
        <p className="mt-1 text-[11px] text-muted-foreground">{t('edit_billing_timezone_hint')}</p>
      </Field>
      {server.renewal && (
        <div className="space-y-1">
          <div className="flex items-center justify-between gap-3">
            <span className="font-medium text-sm">{t('edit_automatic_renewal')}</span>
            <Switch
              aria-describedby={prerequisitesId}
              aria-invalid={state.automaticRenewal && !hasPrerequisites}
              aria-label={t('edit_automatic_renewal')}
              checked={state.automaticRenewal}
              disabled={!(state.automaticRenewal || hasPrerequisites)}
              onCheckedChange={(automaticRenewal) => dispatch({ type: 'patch', value: { automaticRenewal } })}
            />
          </div>
          <p
            className="text-muted-foreground text-xs"
            id={prerequisitesId}
            role={state.automaticRenewal && !hasPrerequisites ? 'alert' : undefined}
          >
            {t('edit_renewal_prerequisites')}
          </p>
          <p className="text-muted-foreground text-xs">{t('renewal_forecast_explanation')}</p>
          <RenewalDeadlineInfo renewal={server.renewal} />
          <p className="text-muted-foreground text-xs">{t('renewal_cost_independent')}</p>
        </div>
      )}
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label={t('edit_traffic_limit')}>
          <Input
            aria-label={t('edit_traffic_limit')}
            autoComplete="off"
            min="0"
            name="traffic_limit"
            onChange={(e) => dispatch({ type: 'patch', value: { trafficLimit: e.target.value } })}
            placeholder={t('edit_unlimited')}
            step="0.1"
            type="number"
            value={state.trafficLimit}
          />
        </Field>
        <Field label={t('edit_limit_type')}>
          <Select
            items={{
              sum: t('edit_limit_total'),
              up: t('edit_limit_upload'),
              down: t('edit_limit_download')
            }}
            onValueChange={(value) => value !== null && dispatch({ type: 'patch', value: { trafficLimitType: value } })}
            value={state.trafficLimitType}
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
          onChange={(e) => dispatch({ type: 'patch', value: { billingStartDay: e.target.value } })}
          placeholder={t('edit_billing_start_day_placeholder', {
            defaultValue: 'Leave empty for natural month (1st)'
          })}
          type="number"
          value={state.billingStartDay}
        />
      </Field>
    </fieldset>
  )
}

function ServerEditDialogContent({ server, onClose }: { onClose: () => void; server: ServerResponse }) {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()
  // The country override field is empty unless the server already has a manual
  // override; otherwise we leave it blank and surface the auto-detected value as
  // a hint, so saving an untouched form never accidentally pins GeoIP.
  const initialCountryCode = server.geo_manual ? (server.country_code ?? '') : ''
  // The draft belongs to the snapshot opened in this dialog. Catalog refreshes
  // may advance the deadline while it stays open; they must not turn untouched
  // draft fields into explicit renewal edits.
  const [initialRenewal] = useState(() => ({
    expiryDate: serverEditStateFromServer(server).expiredAt,
    billingTimezone: server.renewal?.billing_timezone ?? 'UTC',
    enabled: server.renewal?.enabled ?? false
  }))
  const [state, dispatch] = useReducer(serverEditReducer, server, serverEditStateFromServer)
  const invalidAutomaticRenewal = state.automaticRenewal && !hasAutomaticRenewalPrerequisites(state)

  const { data: groups } = useQuery<ServerGroup[]>({
    queryKey: ['server-groups'],
    queryFn: () => api.get<ServerGroup[]>('/api/server-groups'),
    staleTime: 60_000,
    enabled: true
  })

  const { data: initialTags } = useServerTags(server.id, true)
  const tagsMutation = useUpdateServerTags(server.id)
  const tagsInput = state.tagsDraft.dirty ? state.tagsDraft.value : (initialTags?.join(', ') ?? '')

  const mutation = useMutation({
    mutationFn: (payload: UpdateServerInput) => api.put<ServerResponse>(`/api/servers/${server.id}`, payload),
    onSuccess: (data) => {
      projectServerCatalog(queryClient, { kind: 'server_saved', server: data })
      invalidateServerCosts(queryClient, [data.id])
    }
  })

  const buildPayload = (): UpdateServerInput => {
    const renewal = {
      ...(state.automaticRenewal !== initialRenewal.enabled && { enabled: state.automaticRenewal }),
      ...(state.expiredAt !== initialRenewal.expiryDate && { expiry_date: state.expiredAt || null }),
      ...(state.billingTimezone !== initialRenewal.billingTimezone && {
        billing_timezone: state.billingTimezone || null
      })
    }
    const payload: UpdateServerInput = {
      name: state.name,
      weight: state.weight,
      hidden: state.hidden,
      group_id: state.groupId || null,
      remark: state.remark || null,
      public_remark: state.publicRemark || null,
      price: state.price ? Number.parseFloat(state.price) : null,
      billing_cycle: state.billingCycle || null,
      currency: state.currency || null,
      traffic_limit: state.trafficLimit ? Math.round(Number.parseFloat(state.trafficLimit) * 1024 ** 3) : null,
      traffic_limit_type: state.trafficLimitType || null,
      billing_start_day: state.billingStartDay ? Number.parseInt(state.billingStartDay, 10) : null,
      ...(Object.keys(renewal).length > 0 && { renewal }),
      ...countryCodePatch(state.countryCode, initialCountryCode)
    }
    return payload
  }

  const saveTags = async (tags: string[]): Promise<boolean> => {
    try {
      await tagsMutation.mutateAsync(tags)
      return true
    } catch (err) {
      if (initialTags) {
        dispatch({ type: 'patch', value: { tagsDraft: { dirty: false, value: '' } } })
      }
      toast.error(err instanceof Error ? err.message : t('tags_save_failed'))
      return false
    }
  }

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    if (invalidAutomaticRenewal) {
      return
    }
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
    if (state.tagsDraft.dirty && !(await saveTags(parsed.tags))) {
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
          <ServerEditBasicFields
            dispatch={dispatch}
            groups={groups}
            server={server}
            state={state}
            t={t}
            tagsInput={tagsInput}
          />
          <ServerEditBillingFields dispatch={dispatch} server={server} state={state} t={t} />

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
          <Button disabled={mutation.isPending || tagsMutation.isPending || invalidAutomaticRenewal} type="submit">
            {mutation.isPending || tagsMutation.isPending ? t('common:saving') : t('common:save')}
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  )
}

function hasAutomaticRenewalPrerequisites(state: ServerEditState): boolean {
  if (!(state.expiredAt && ['monthly', 'quarterly', 'yearly'].includes(state.billingCycle) && state.billingTimezone)) {
    return false
  }
  try {
    // Numeric UTC offsets are not IANA timezone identifiers.
    if (state.billingTimezone.startsWith('+') || state.billingTimezone.startsWith('-')) {
      return false
    }
    new Intl.DateTimeFormat('en', { timeZone: state.billingTimezone }).format()
    return true
  } catch {
    return false
  }
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

interface ExpiryDateFieldProps {
  ariaLabel: string
  onChange: (value: string) => void
  value: string
}

function ExpiryDateField({ ariaLabel, onChange, value }: ExpiryDateFieldProps) {
  const { t } = useTranslation('servers')
  return (
    <div>
      <Input
        aria-label={ariaLabel}
        name="expiry_date"
        onChange={(e) => onChange(e.target.value)}
        type="date"
        value={value}
      />
      {value && (
        <Button className="mt-1" onClick={() => onChange('')} size="sm" type="button" variant="ghost">
          {t('edit_expiration_clear')}
        </Button>
      )}
    </div>
  )
}
