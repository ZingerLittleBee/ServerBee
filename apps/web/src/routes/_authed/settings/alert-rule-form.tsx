import { ChevronDown, Plus, Trash2 } from 'lucide-react'
import { type Dispatch, type FormEvent, useMemo, useReducer } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { CollapsibleContent } from '@/components/ui/collapsible-content'
import { Collapsible } from '@/components/ui/collapsible-root'
import { CollapsibleTrigger } from '@/components/ui/collapsible-trigger'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import type { AlertRuleItem, NotificationGroup } from '@/lib/api-schema'

interface BlockSourceIpAction {
  comment?: string | null
  cover_type: string
  server_ids_json?: string | null
  type: 'block_source_ip'
}

interface Server {
  id: string
  name: string
}

type CoverType = 'all' | 'exclude' | 'include'

export interface CreateAlertRuleInput {
  actions?: BlockSourceIpAction[]
  cover_type: string
  name: string
  notification_group_id: string | null
  rules: AlertRuleItem[]
  server_ids: string[]
  trigger_mode: string
}

interface AlertRuleFormState {
  autoBlockComment: string
  autoBlockCoverType: CoverType
  autoBlockEnabled: boolean
  autoBlockServerIds: string[]
  coverType: CoverType
  groupId: string
  name: string
  ruleItems: AlertRuleItem[]
  serverIds: string[]
  triggerMode: string
}

interface RuleTypeOption {
  label: string
  value: string
}

type AlertRuleFormAction =
  | { type: 'add-rule-item' }
  | { checked: boolean; serverId: string; type: 'toggle-auto-block-server' }
  | { checked: boolean; serverId: string; type: 'toggle-server' }
  | { index: number; type: 'remove-rule-item' }
  | { field: keyof AlertRuleItem; index: number; type: 'update-rule-item'; value: number | string }
  | { type: 'set-auto-block-comment'; value: string }
  | { type: 'set-auto-block-cover-type'; value: CoverType }
  | { type: 'set-auto-block-enabled'; value: boolean }
  | { type: 'set-cover-type'; value: CoverType }
  | { type: 'set-group-id'; value: string }
  | { type: 'set-name'; value: string }
  | { type: 'set-trigger-mode'; value: string }

type AlertRuleFormDispatch = Dispatch<AlertRuleFormAction>

const AUTO_BLOCK_RULE_TYPES = new Set(['ssh_brute_force_detected', 'port_scan_detected'])
const THRESHOLD_TYPES = new Set([
  'cpu',
  'memory',
  'swap',
  'disk',
  'load1',
  'load5',
  'load15',
  'tcp_conn',
  'udp_conn',
  'process',
  'net_in_speed',
  'net_out_speed',
  'temperature',
  'gpu',
  'network_latency',
  'network_packet_loss'
])
const CYCLE_TYPES = new Set(['transfer_in_cycle', 'transfer_out_cycle', 'transfer_all_cycle'])

const DEFAULT_STATE: AlertRuleFormState = {
  autoBlockComment: '',
  autoBlockCoverType: 'all',
  autoBlockEnabled: false,
  autoBlockServerIds: [],
  coverType: 'all',
  groupId: '',
  name: '',
  ruleItems: [{ rule_type: 'cpu', min: 90 }],
  serverIds: [],
  triggerMode: 'always'
}

function updateSelectedIds(ids: string[], id: string, checked: boolean): string[] {
  if (checked) {
    return ids.includes(id) ? ids : [...ids, id]
  }

  return ids.filter((currentId) => currentId !== id)
}

function alertRuleFormReducer(state: AlertRuleFormState, action: AlertRuleFormAction): AlertRuleFormState {
  switch (action.type) {
    case 'add-rule-item':
      return { ...state, ruleItems: [...state.ruleItems, { rule_type: 'cpu', min: 90 }] }
    case 'remove-rule-item':
      return { ...state, ruleItems: state.ruleItems.filter((_, index) => index !== action.index) }
    case 'set-auto-block-comment':
      return { ...state, autoBlockComment: action.value }
    case 'set-auto-block-cover-type':
      return {
        ...state,
        autoBlockCoverType: action.value,
        autoBlockServerIds: action.value === 'all' ? [] : state.autoBlockServerIds
      }
    case 'set-auto-block-enabled':
      return { ...state, autoBlockEnabled: action.value }
    case 'set-cover-type':
      return {
        ...state,
        coverType: action.value,
        serverIds: action.value === 'all' ? [] : state.serverIds
      }
    case 'set-group-id':
      return { ...state, groupId: action.value }
    case 'set-name':
      return { ...state, name: action.value }
    case 'set-trigger-mode':
      return { ...state, triggerMode: action.value }
    case 'toggle-auto-block-server':
      return {
        ...state,
        autoBlockServerIds: updateSelectedIds(state.autoBlockServerIds, action.serverId, action.checked)
      }
    case 'toggle-server':
      return { ...state, serverIds: updateSelectedIds(state.serverIds, action.serverId, action.checked) }
    case 'update-rule-item':
      return {
        ...state,
        ruleItems: state.ruleItems.map((item, index) =>
          index === action.index ? { ...item, [action.field]: action.value } : item
        )
      }
    default:
      return state
  }
}

export function AlertRuleForm({
  createPending,
  groups,
  onCancel,
  onSubmit,
  servers
}: {
  createPending: boolean
  groups: NotificationGroup[] | undefined
  onCancel: () => void
  onSubmit: (input: CreateAlertRuleInput) => void
  servers: Server[] | undefined
}) {
  const { t } = useTranslation(['settings', 'common'])
  const [state, dispatch] = useReducer(alertRuleFormReducer, DEFAULT_STATE)
  const autoBlockEligible = useMemo(
    () => state.ruleItems.length > 0 && state.ruleItems.every((item) => AUTO_BLOCK_RULE_TYPES.has(item.rule_type)),
    [state.ruleItems]
  )
  const ruleTypes: RuleTypeOption[] = [
    { label: t('alerts.metric_cpu'), value: 'cpu' },
    { label: t('alerts.metric_memory'), value: 'memory' },
    { label: t('alerts.metric_swap'), value: 'swap' },
    { label: t('alerts.metric_disk'), value: 'disk' },
    { label: t('alerts.metric_load1'), value: 'load1' },
    { label: t('alerts.metric_load5'), value: 'load5' },
    { label: t('alerts.metric_load15'), value: 'load15' },
    { label: t('alerts.metric_tcp'), value: 'tcp_conn' },
    { label: t('alerts.metric_udp'), value: 'udp_conn' },
    { label: t('alerts.metric_processes'), value: 'process' },
    { label: t('alerts.metric_net_in'), value: 'net_in_speed' },
    { label: t('alerts.metric_net_out'), value: 'net_out_speed' },
    { label: t('alerts.metric_temperature'), value: 'temperature' },
    { label: t('alerts.metric_gpu'), value: 'gpu' },
    { label: t('alerts.metric_offline'), value: 'offline' },
    { label: t('alerts.metric_transfer_in'), value: 'transfer_in_cycle' },
    { label: t('alerts.metric_transfer_out'), value: 'transfer_out_cycle' },
    { label: t('alerts.metric_transfer_total'), value: 'transfer_all_cycle' },
    { label: t('alerts.metric_expiration'), value: 'expiration' },
    { label: 'Network Latency', value: 'network_latency' },
    { label: 'Network Packet Loss', value: 'network_packet_loss' },
    { label: 'IP Changed', value: 'ip_changed' },
    { label: t('alerts.metric_capability_granted'), value: 'capability_grant_detected' }
  ]

  const handleCreate = (event: FormEvent) => {
    event.preventDefault()
    if (state.name.trim().length === 0) {
      toast.error(t('alerts.name_required'))
      return
    }
    if (state.ruleItems.length === 0) {
      toast.error(t('alerts.rules_required'))
      return
    }

    const actions: BlockSourceIpAction[] =
      autoBlockEligible && state.autoBlockEnabled
        ? [
            {
              type: 'block_source_ip',
              cover_type: state.autoBlockCoverType,
              server_ids_json:
                state.autoBlockCoverType === 'all' || state.autoBlockServerIds.length === 0
                  ? null
                  : JSON.stringify(state.autoBlockServerIds),
              comment: state.autoBlockComment.trim().length > 0 ? state.autoBlockComment.trim() : null
            }
          ]
        : []

    onSubmit({
      name: state.name.trim(),
      trigger_mode: state.triggerMode,
      notification_group_id: state.groupId || null,
      rules: state.ruleItems,
      cover_type: state.coverType,
      server_ids: state.coverType === 'include' || state.coverType === 'exclude' ? state.serverIds : [],
      actions: actions.length > 0 ? actions : undefined
    })
  }

  return (
    <form className="mb-4 space-y-3 rounded-md border bg-muted/30 p-4" onSubmit={handleCreate}>
      <AlertRuleBasicsSection
        dispatch={dispatch}
        groupId={state.groupId}
        groups={groups}
        name={state.name}
        triggerMode={state.triggerMode}
      />
      <AlertRuleScopeSection
        coverType={state.coverType}
        dispatch={dispatch}
        selectedServerIds={state.serverIds}
        servers={servers}
      />
      <AlertRuleConditionsSection dispatch={dispatch} ruleItems={state.ruleItems} ruleTypes={ruleTypes} />
      {autoBlockEligible && <AlertRuleAutoBlockSection dispatch={dispatch} servers={servers} state={state} />}
      <div className="flex gap-2">
        <Button disabled={createPending} size="sm" type="submit">
          {t('alerts.create_rule')}
        </Button>
        <Button onClick={onCancel} size="sm" type="button" variant="ghost">
          {t('common:cancel')}
        </Button>
      </div>
    </form>
  )
}

function AlertRuleBasicsSection({
  dispatch,
  groupId,
  groups,
  name,
  triggerMode
}: {
  dispatch: AlertRuleFormDispatch
  groupId: string
  groups: NotificationGroup[] | undefined
  name: string
  triggerMode: string
}) {
  const { t } = useTranslation(['settings', 'common'])

  return (
    <>
      <Input
        aria-label={t('alerts.rule_name')}
        onChange={(event) => dispatch({ type: 'set-name', value: event.target.value })}
        placeholder={t('alerts.rule_name')}
        required
        type="text"
        value={name}
      />
      <div className="flex flex-col gap-3 sm:flex-row">
        <Select
          items={{ always: t('alerts.trigger_always'), once: t('alerts.trigger_once') }}
          onValueChange={(value) => value !== null && dispatch({ type: 'set-trigger-mode', value })}
          value={triggerMode}
        >
          <SelectTrigger aria-label={t('alerts.trigger_always')} className="h-9 w-full flex-1">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="always">{t('alerts.trigger_always')}</SelectItem>
            <SelectItem value="once">{t('alerts.trigger_once')}</SelectItem>
          </SelectContent>
        </Select>
        <Select
          items={[
            { value: '', label: t('alerts.no_notification') },
            ...(groups?.map((group) => ({ value: group.id, label: group.name })) ?? [])
          ]}
          onValueChange={(value) => dispatch({ type: 'set-group-id', value: value ?? '' })}
          value={groupId}
        >
          <SelectTrigger aria-label={t('alerts.no_notification')} className="h-9 w-full flex-1">
            <SelectValue placeholder={t('alerts.no_notification')} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="">{t('alerts.no_notification')}</SelectItem>
            {groups?.map((group) => (
              <SelectItem key={group.id} value={group.id}>
                {group.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    </>
  )
}

function AlertRuleScopeSection({
  coverType,
  dispatch,
  selectedServerIds,
  servers
}: {
  coverType: CoverType
  dispatch: AlertRuleFormDispatch
  selectedServerIds: string[]
  servers: Server[] | undefined
}) {
  const { t } = useTranslation(['settings', 'common'])

  return (
    <div className="space-y-2">
      <span className="font-medium text-sm">{t('alerts.coverage')}</span>
      <CoverageSelect
        ariaLabel={t('alerts.coverage')}
        onChange={(value) => dispatch({ type: 'set-cover-type', value })}
        value={coverType}
      />
      {(coverType === 'include' || coverType === 'exclude') && (
        <ServerCheckboxList
          emptyLabel={t('alerts.no_servers')}
          onToggle={(serverId, checked) => dispatch({ checked, serverId, type: 'toggle-server' })}
          selectedIds={selectedServerIds}
          servers={servers}
        />
      )}
    </div>
  )
}

function AlertRuleConditionsSection({
  dispatch,
  ruleItems,
  ruleTypes
}: {
  dispatch: AlertRuleFormDispatch
  ruleItems: AlertRuleItem[]
  ruleTypes: RuleTypeOption[]
}) {
  const { t } = useTranslation(['settings', 'common'])

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <span className="font-medium text-sm">{t('alerts.conditions')}</span>
        <Button onClick={() => dispatch({ type: 'add-rule-item' })} size="sm" type="button" variant="ghost">
          <Plus className="size-3" />
          {t('alerts.add_condition')}
        </Button>
      </div>
      {ruleItems.map((item, index) => (
        <AlertRuleConditionRow
          canRemove={ruleItems.length > 1}
          dispatch={dispatch}
          index={index}
          item={item}
          key={`rule-${index.toString()}`}
          ruleTypes={ruleTypes}
        />
      ))}
    </div>
  )
}

function AlertRuleConditionRow({
  canRemove,
  dispatch,
  index,
  item,
  ruleTypes
}: {
  canRemove: boolean
  dispatch: AlertRuleFormDispatch
  index: number
  item: AlertRuleItem
  ruleTypes: RuleTypeOption[]
}) {
  const { t } = useTranslation(['settings', 'common'])

  return (
    <div className="flex gap-2">
      <Select
        items={ruleTypes}
        onValueChange={(value) =>
          value !== null && dispatch({ field: 'rule_type', index, type: 'update-rule-item', value })
        }
        value={item.rule_type}
      >
        <SelectTrigger aria-label={t('alerts.conditions')} className="h-9 w-full flex-1">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {ruleTypes.map((ruleType) => (
            <SelectItem key={ruleType.value} value={ruleType.value}>
              {ruleType.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {THRESHOLD_TYPES.has(item.rule_type) && <ThresholdInputs dispatch={dispatch} index={index} item={item} />}
      {item.rule_type === 'offline' && (
        <NumberRuleInput
          ariaLabel={t('alerts.duration')}
          field="duration"
          index={index}
          onChange={dispatch}
          placeholder={t('alerts.duration')}
          value={item.duration ?? 60}
          valueFallback={60}
        />
      )}
      {item.rule_type === 'expiration' && (
        <NumberRuleInput
          ariaLabel={t('alerts.days_before')}
          field="duration"
          index={index}
          onChange={dispatch}
          placeholder={t('alerts.days_before')}
          value={item.duration ?? 7}
          valueFallback={7}
        />
      )}
      {CYCLE_TYPES.has(item.rule_type) && <CycleInputs dispatch={dispatch} index={index} item={item} />}
      {canRemove && (
        <Button
          aria-label={t('common:delete')}
          onClick={() => dispatch({ index, type: 'remove-rule-item' })}
          size="sm"
          type="button"
          variant="ghost"
        >
          <Trash2 aria-hidden="true" className="size-3" />
        </Button>
      )}
    </div>
  )
}

function ThresholdInputs({
  dispatch,
  index,
  item
}: {
  dispatch: AlertRuleFormDispatch
  index: number
  item: AlertRuleItem
}) {
  const { t } = useTranslation(['settings', 'common'])

  return (
    <>
      <NumberRuleInput
        ariaLabel={t('alerts.threshold_gte')}
        field="min"
        index={index}
        onChange={dispatch}
        placeholder={t('alerts.threshold_gte')}
        value={item.min ?? ''}
        valueFallback={0}
      />
      <NumberRuleInput
        ariaLabel={t('alerts.threshold_lte')}
        field="max"
        index={index}
        onChange={dispatch}
        placeholder={t('alerts.threshold_lte')}
        value={item.max ?? ''}
        valueFallback={0}
      />
    </>
  )
}

function CycleInputs({
  dispatch,
  index,
  item
}: {
  dispatch: AlertRuleFormDispatch
  index: number
  item: AlertRuleItem
}) {
  const { t } = useTranslation(['settings', 'common'])

  return (
    <>
      <Select
        items={{
          hour: t('alerts.period_hour'),
          day: t('alerts.period_day'),
          week: t('alerts.period_week'),
          month: t('alerts.period_month'),
          year: t('alerts.period_year')
        }}
        onValueChange={(value) =>
          value !== null && dispatch({ field: 'cycle_interval', index, type: 'update-rule-item', value })
        }
        value={item.cycle_interval ?? 'month'}
      >
        <SelectTrigger aria-label={t('alerts.period_month')} className="h-9 w-28">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="hour">{t('alerts.period_hour')}</SelectItem>
          <SelectItem value="day">{t('alerts.period_day')}</SelectItem>
          <SelectItem value="week">{t('alerts.period_week')}</SelectItem>
          <SelectItem value="month">{t('alerts.period_month')}</SelectItem>
          <SelectItem value="year">{t('alerts.period_year')}</SelectItem>
        </SelectContent>
      </Select>
      <NumberRuleInput
        ariaLabel={t('alerts.limit_bytes')}
        field="cycle_limit"
        index={index}
        onChange={dispatch}
        placeholder={t('alerts.limit_bytes')}
        value={item.cycle_limit ?? ''}
        valueFallback={0}
      />
    </>
  )
}

function NumberRuleInput({
  ariaLabel,
  field,
  index,
  onChange,
  placeholder,
  value,
  valueFallback
}: {
  ariaLabel: string
  field: keyof AlertRuleItem
  index: number
  onChange: AlertRuleFormDispatch
  placeholder: string
  value: number | string
  valueFallback: number
}) {
  return (
    <Input
      aria-label={ariaLabel}
      className="w-28"
      onChange={(event) =>
        onChange({
          field,
          index,
          type: 'update-rule-item',
          value: Number.parseFloat(event.target.value) || valueFallback
        })
      }
      placeholder={placeholder}
      type="number"
      value={value}
    />
  )
}

function AlertRuleAutoBlockSection({
  dispatch,
  servers,
  state
}: {
  dispatch: AlertRuleFormDispatch
  servers: Server[] | undefined
  state: AlertRuleFormState
}) {
  const { t } = useTranslation(['settings', 'common'])

  return (
    <Collapsible className="rounded-md border bg-background p-3" open={state.autoBlockEnabled}>
      <CollapsibleTrigger
        render={
          <button
            aria-label={t('alerts.auto_block_title', { defaultValue: 'Auto-block source IP' })}
            className="flex w-full items-center justify-between text-left"
            onClick={() => dispatch({ type: 'set-auto-block-enabled', value: !state.autoBlockEnabled })}
            type="button"
          />
        }
      >
        <span className="flex items-center gap-2">
          <Checkbox
            checked={state.autoBlockEnabled}
            onCheckedChange={(checked) => dispatch({ type: 'set-auto-block-enabled', value: checked === true })}
          />
          <span className="font-medium text-sm">
            {t('alerts.auto_block_title', { defaultValue: 'Auto-block source IP' })}
          </span>
        </span>
        <ChevronDown aria-hidden="true" className="size-4 text-muted-foreground" />
      </CollapsibleTrigger>
      <CollapsibleContent className="mt-3 space-y-3">
        <p className="text-muted-foreground text-xs">
          {t('alerts.auto_block_hint', {
            defaultValue:
              'When the rule fires, push a block_source_ip action that drops the offending IP via the firewall.'
          })}
        </p>
        <div className="space-y-2">
          <CoverageSelect
            ariaLabel={t('alerts.auto_block_scope', { defaultValue: 'Auto-block scope' })}
            onChange={(value) => dispatch({ type: 'set-auto-block-cover-type', value })}
            value={state.autoBlockCoverType}
          />
          {(state.autoBlockCoverType === 'include' || state.autoBlockCoverType === 'exclude') && (
            <ServerCheckboxList
              emptyLabel={t('alerts.no_servers')}
              onToggle={(serverId, checked) => dispatch({ checked, serverId, type: 'toggle-auto-block-server' })}
              selectedIds={state.autoBlockServerIds}
              servers={servers}
            />
          )}
          <Textarea
            aria-label={t('alerts.auto_block_comment', { defaultValue: 'Auto-block comment' })}
            onChange={(event) => dispatch({ type: 'set-auto-block-comment', value: event.target.value })}
            placeholder={t('alerts.auto_block_comment_placeholder', {
              defaultValue: 'Reason added to every auto-created block (optional)'
            })}
            rows={2}
            value={state.autoBlockComment}
          />
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}

function CoverageSelect({
  ariaLabel,
  onChange,
  value
}: {
  ariaLabel: string
  onChange: (value: CoverType) => void
  value: CoverType
}) {
  const { t } = useTranslation(['settings', 'common'])

  return (
    <Select
      items={{
        all: t('alerts.all_servers'),
        include: t('alerts.include_servers'),
        exclude: t('alerts.exclude_servers')
      }}
      onValueChange={(nextValue) => {
        if (nextValue !== null) {
          onChange(nextValue as CoverType)
        }
      }}
      value={value}
    >
      <SelectTrigger aria-label={ariaLabel} className="h-9 w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="all">{t('alerts.all_servers')}</SelectItem>
        <SelectItem value="include">{t('alerts.include_servers')}</SelectItem>
        <SelectItem value="exclude">{t('alerts.exclude_servers')}</SelectItem>
      </SelectContent>
    </Select>
  )
}

function ServerCheckboxList({
  emptyLabel,
  onToggle,
  selectedIds,
  servers
}: {
  emptyLabel: string
  onToggle: (serverId: string, checked: boolean) => void
  selectedIds: string[]
  servers: Server[] | undefined
}) {
  return (
    <div className="flex flex-wrap gap-2 rounded-md border p-2">
      {servers && servers.length > 0 ? (
        servers.map((server) => (
          // biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element
          <label className="flex items-center gap-1.5 text-sm" key={server.id}>
            <Checkbox
              checked={selectedIds.includes(server.id)}
              onCheckedChange={(checked) => onToggle(server.id, checked === true)}
            />
            {server.name}
          </label>
        ))
      ) : (
        <span className="text-muted-foreground text-xs">{emptyLabel}</span>
      )}
    </div>
  )
}
