import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { TFunction } from 'i18next'
import { CalendarIcon, Copy, Plus } from 'lucide-react'
import { type FormEvent, useReducer } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import {
  AgentCapabilityPicker,
  ALL_AGENT_CAPABILITY_KEYS,
  DEFAULT_AGENT_CAPABILITY_KEYS,
  resolveAgentCapabilitySelection
} from '@/components/server/agent-capability-picker'
import { Button } from '@/components/ui/button'
import { Calendar } from '@/components/ui/calendar'
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { api } from '@/lib/api-client'
import type {
  CreateServerRequest,
  CreateServerResponse,
  EnrollmentIssueResponse,
  EnrollmentOfferResponse,
  ServerGroup
} from '@/lib/api-schema'
import { refreshServerCatalog } from '@/lib/server-catalog'
import { cn } from '@/lib/utils'
import { randomUUID } from '@/lib/uuid'

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

interface AddServerFormState {
  billingCycle: string
  billingStartDay: string
  currency: string
  expiredAt: string
  groupId: string
  issued: CreateServerResponse | null
  name: string
  onboardingRequestId: string
  price: string
  publicRemark: string
  remark: string
  selectedCaps: Set<string>
  tagsInput: string
  trafficLimit: string
  trafficLimitType: string
}

type AddServerFormAction =
  | { type: 'patch'; value: Partial<AddServerFormState> }
  | { type: 'reset' }
  | { type: 'setCaps'; value: Set<string> }
  | { type: 'setIssued'; value: CreateServerResponse | null }
  | { type: 'toggleCap'; key: string }

function initialAddServerFormState(): AddServerFormState {
  return {
    billingCycle: '',
    billingStartDay: '',
    currency: 'USD',
    expiredAt: '',
    groupId: '',
    issued: null,
    name: '',
    onboardingRequestId: randomUUID(),
    price: '',
    publicRemark: '',
    remark: '',
    selectedCaps: new Set(DEFAULT_AGENT_CAPABILITY_KEYS),
    tagsInput: '',
    trafficLimit: '',
    trafficLimitType: 'sum'
  }
}

function addServerFormReducer(state: AddServerFormState, action: AddServerFormAction): AddServerFormState {
  switch (action.type) {
    case 'patch':
      return { ...state, ...action.value }
    case 'reset':
      return initialAddServerFormState()
    case 'setCaps':
      return { ...state, selectedCaps: action.value }
    case 'setIssued':
      return { ...state, issued: action.value }
    case 'toggleCap': {
      const selectedCaps = new Set(state.selectedCaps)
      if (selectedCaps.has(action.key)) {
        selectedCaps.delete(action.key)
      } else {
        selectedCaps.add(action.key)
      }
      return { ...state, selectedCaps }
    }
    default:
      return state
  }
}

function AddServerIssuedView({
  enrollment,
  installCommand,
  onAnother,
  onClose,
  onCopy,
  t
}: {
  installCommand: string
  enrollment: EnrollmentIssueResponse
  onAnother: () => void
  onClose: () => void
  onCopy: (value: string) => void
  t: TFunction
}) {
  return (
    <>
      <DialogBody className="space-y-5">
        <p className="text-muted-foreground text-sm">{t('add_server.description')}</p>

        <div className="space-y-4 rounded-md border border-amber-500/40 bg-amber-500/5 p-4">
          <p className="text-amber-600 text-sm dark:text-amber-500">{t('add_server.shown_once_warning')}</p>

          <div>
            <p className="mb-1 font-medium text-muted-foreground text-xs">{t('add_server.code_label')}</p>
            <div className="flex min-w-0 items-center gap-2">
              <code className="min-w-0 flex-1 truncate rounded-md border bg-muted/50 px-3 py-2 font-mono text-sm">
                {enrollment.code}
              </code>
              <Button
                aria-label={t('add_server.copy')}
                onClick={() => onCopy(enrollment.code)}
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
                onClick={() => onCopy(installCommand)}
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
        <Button onClick={onAnother} type="button" variant="outline">
          {t('add_server.another')}
        </Button>
        <Button onClick={onClose} type="button">
          {t('add_server.done')}
        </Button>
      </DialogFooter>
    </>
  )
}

function AddServerBasicFields({
  dispatch,
  groups,
  state,
  t
}: {
  dispatch: (action: AddServerFormAction) => void
  groups: ServerGroup[] | undefined
  state: AddServerFormState
  t: TFunction
}) {
  return (
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
          onChange={(e) => dispatch({ type: 'patch', value: { name: e.target.value } })}
          placeholder={t('add_server.name_placeholder')}
          required
          type="text"
          value={state.name}
        />
      </Field>
      <Field label={t('add_server.group_label')}>
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
      <Field label={t('add_server.tags_label')}>
        <Input
          aria-label={t('add_server.tags_label')}
          autoComplete="off"
          name="tags"
          onChange={(e) => dispatch({ type: 'patch', value: { tagsInput: e.target.value } })}
          placeholder={t('tags_placeholder')}
          type="text"
          value={state.tagsInput}
        />
        <p className="mt-1 text-[11px] text-muted-foreground">{t('tags_hint')}</p>
      </Field>
      <Field label={t('add_server.remark_label')}>
        <Input
          aria-label={t('add_server.remark_label')}
          autoComplete="off"
          name="remark"
          onChange={(e) => dispatch({ type: 'patch', value: { remark: e.target.value } })}
          placeholder={t('edit_remark_placeholder')}
          type="text"
          value={state.remark}
        />
      </Field>
      <Field label={t('add_server.public_remark_label')}>
        <Input
          aria-label={t('add_server.public_remark_label')}
          autoComplete="off"
          name="public_remark"
          onChange={(e) => dispatch({ type: 'patch', value: { publicRemark: e.target.value } })}
          placeholder={t('edit_public_remark_placeholder')}
          type="text"
          value={state.publicRemark}
        />
      </Field>
    </fieldset>
  )
}

function AddServerBillingFields({
  dispatch,
  state,
  t
}: {
  dispatch: (action: AddServerFormAction) => void
  state: AddServerFormState
  t: TFunction
}) {
  return (
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
            onChange={(e) => dispatch({ type: 'patch', value: { price: e.target.value } })}
            placeholder="0.00"
            step="0.01"
            type="number"
            value={state.price}
          />
        </Field>
        <Field label={t('add_server.currency_label')}>
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
        <Field label={t('add_server.billing_cycle_label')}>
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
      <Field label={t('add_server.expired_at_label')}>
        <DatePickerField
          ariaLabel={t('add_server.expired_at_label')}
          onChange={(expiredAt) => dispatch({ type: 'patch', value: { expiredAt } })}
          value={state.expiredAt}
        />
      </Field>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label={t('add_server.traffic_limit_label')}>
          <Input
            aria-label={t('add_server.traffic_limit_label')}
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
        <Field label={t('add_server.traffic_limit_type_label')}>
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
      <Field label={t('add_server.billing_start_day_label')}>
        <Input
          aria-label={t('add_server.billing_start_day_label')}
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

export function AddServerDialog({ open, onClose }: { onClose: () => void; open: boolean }) {
  const { t } = useTranslation(['servers', 'common'])
  const queryClient = useQueryClient()
  const [state, dispatch] = useReducer(addServerFormReducer, undefined, initialAddServerFormState)

  const { data: groups } = useQuery<ServerGroup[]>({
    queryKey: ['server-groups'],
    queryFn: () => api.get<ServerGroup[]>('/api/server-groups'),
    staleTime: 60_000,
    enabled: open
  })

  const mutation = useMutation({
    mutationFn: (body: CreateServerRequest) => api.post<CreateServerResponse>('/api/servers', body),
    onSuccess: async (data) => {
      dispatch({ type: 'setIssued', value: data })
      try {
        await refreshServerCatalog(queryClient)
      } catch {
        // Best-effort: the new row will surface on the next WS full_sync.
      }
    },
    onError: (err: unknown) => {
      // Backend `AppError` messages are English-only, so keep a localized
      // headline and demote the raw server text to the description.
      toast.error(t('add_server.generate_failed'), {
        description: err instanceof Error ? err.message : undefined
      })
    }
  })

  const replaceReplayOfferMutation = useMutation({
    mutationFn: ({ offerId, serverId }: { offerId: string; serverId: string }) =>
      api.post<EnrollmentOfferResponse>(`/api/servers/${serverId}/agent-authority/offers/${offerId}/replace`, {}),
    onSuccess: (data) => {
      const current = state.issued
      if (!current) {
        return
      }
      dispatch({
        type: 'setIssued',
        value: {
          ...current,
          enrollment: data.enrollment,
          outstanding_offer: {
            id: data.enrollment.id,
            code_prefix: data.enrollment.code_prefix,
            created_at: new Date().toISOString(),
            expires_at: data.enrollment.expires_at
          }
        }
      })
    },
    onError: (err: unknown) => {
      toast.error(t('add_server.replace_failed'), {
        description: err instanceof Error ? err.message : undefined
      })
    }
  })

  const origin = typeof window !== 'undefined' ? window.location.origin : ''
  // Emit --caps only when the selection differs from the default set; an
  // omitted flag means "use install.sh's built-in defaults", which keeps the
  // copy/paste command short for the common case.
  const capabilitySelection = resolveAgentCapabilitySelection(state.selectedCaps)
  const issued = state.issued
  const installCommand = issued?.enrollment
    ? `curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- agent --server-url '${origin}' --enrollment-code '${issued.enrollment.code}'${capabilitySelection.installArgument}`
    : ''

  const toggleCap = (key: string) => {
    dispatch({ type: 'toggleCap', key })
  }
  const resetCapsToDefault = () => dispatch({ type: 'setCaps', value: new Set(DEFAULT_AGENT_CAPABILITY_KEYS) })
  const selectAllCaps = () => dispatch({ type: 'setCaps', value: new Set(ALL_AGENT_CAPABILITY_KEYS) })
  const selectNoCaps = () => dispatch({ type: 'setCaps', value: new Set() })

  const copy = async (value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      toast.success(t('add_server.copied'))
    } catch {
      // Clipboard access denied
    }
  }

  const reset = () => {
    dispatch({ type: 'reset' })
  }

  const handleClose = () => {
    reset()
    onClose()
  }

  const buildBody = (trimmedName: string, tags: string[]): CreateServerRequest => {
    const trafficLimitValue = numberOrUndefined(Math.round(parseFloatOrNaN(state.trafficLimit) * 1024 ** 3))
    return {
      onboarding_request_id: state.onboardingRequestId,
      name: trimmedName,
      group_id: state.groupId || undefined,
      tags: tags.length > 0 ? tags : undefined,
      remark: nullIfBlank(state.remark),
      public_remark: nullIfBlank(state.publicRemark),
      price: numberOrUndefined(parseFloatOrNaN(state.price)),
      currency: state.currency || undefined,
      billing_cycle: state.billingCycle || undefined,
      billing_start_day: numberOrUndefined(parseIntOrNaN(state.billingStartDay)),
      expired_at: state.expiredAt ? `${state.expiredAt}T00:00:00Z` : undefined,
      traffic_limit: trafficLimitValue,
      traffic_limit_type: trafficLimitValue === undefined ? undefined : state.trafficLimitType,
      caps: capabilitySelection.isDefault ? undefined : capabilitySelection.keys
    }
  }

  const handleSubmit = (e?: FormEvent) => {
    e?.preventDefault()
    const trimmedName = state.name.trim()
    if (!trimmedName) {
      return
    }
    const parsed = parseTagsInput(state.tagsInput)
    if (parsed.error) {
      toast.error(t(parsed.error))
      return
    }
    mutation.mutate(buildBody(trimmedName, parsed.tags))
  }

  const submitDisabled = mutation.isPending || !state.name.trim()
  const replayOffer = issued?.outstanding_offer ?? null

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

        {issued?.enrollment && (
          <AddServerIssuedView
            enrollment={issued.enrollment}
            installCommand={installCommand}
            onAnother={reset}
            onClose={handleClose}
            onCopy={copy}
            t={t}
          />
        )}
        {!issued?.enrollment && issued?.replayed && (
          <>
            <DialogBody className="space-y-4">
              <p className="text-muted-foreground text-sm">
                {t('add_server.replayed', { serverId: issued.server_id })}
              </p>
              {replayOffer ? (
                <div className="space-y-3 rounded-md border border-amber-500/40 bg-amber-500/5 p-4">
                  <p className="text-amber-700 text-sm dark:text-amber-400">{t('add_server.replay_code_lost')}</p>
                  <Button
                    disabled={replaceReplayOfferMutation.isPending}
                    onClick={() =>
                      replaceReplayOfferMutation.mutate({
                        offerId: replayOffer.id,
                        serverId: issued.server_id
                      })
                    }
                    type="button"
                  >
                    {t('add_server.replace_offer')}
                  </Button>
                </div>
              ) : (
                <p className="text-muted-foreground text-sm">{t('add_server.replay_no_offer')}</p>
              )}
            </DialogBody>
            <DialogFooter>
              <Button onClick={handleClose} type="button">
                {t('add_server.done')}
              </Button>
            </DialogFooter>
          </>
        )}
        {!(issued?.enrollment || issued?.replayed) && (
          <form className="flex min-h-0 flex-1 flex-col gap-4" onSubmit={handleSubmit}>
            <DialogBody className="space-y-4">
              <p className="text-muted-foreground text-sm">{t('add_server.description')}</p>

              <AddServerBasicFields dispatch={dispatch} groups={groups} state={state} t={t} />
              <AddServerBillingFields dispatch={dispatch} state={state} t={t} />
              <AgentCapabilityPicker
                hintKey="add_server.caps_hint"
                idPrefix="add-server-cap"
                labelKey="add_server.caps_label"
                onReset={resetCapsToDefault}
                onSelectAll={selectAllCaps}
                onSelectNone={selectNoCaps}
                onToggle={toggleCap}
                selected={state.selectedCaps}
                t={t}
              />

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
