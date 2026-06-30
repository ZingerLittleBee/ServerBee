import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Bell, Pencil, Plus, Send, Trash2, Upload } from 'lucide-react'
import { type ChangeEvent, type Dispatch, type FormEvent, type RefObject, useReducer, useRef } from 'react'
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
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { api } from '@/lib/api-client'
import type { Notification } from '@/lib/api-schema'
import { buildEmailPayload } from './notification-payloads'

type NotifyType = 'apns' | 'bark' | 'email' | 'telegram' | 'webhook'

interface ChannelFormState {
  configFields: Record<string, string>
  deleteChannelId: string | null
  editingId: string | null
  isEnabled: boolean
  name: string
  notifyType: NotifyType
  showForm: boolean
  toAddresses: string[]
  toInput: string
}

type ChannelFormAction =
  | { type: 'add-recipient'; value: string }
  | { notification: Notification; type: 'edit-channel' }
  | { type: 'prepare-create' }
  | { type: 'remove-recipient'; value: string }
  | { type: 'reset-form' }
  | { type: 'set-config-fields'; value: Record<string, string> }
  | { type: 'set-delete-channel-id'; value: string | null }
  | { type: 'set-enabled'; value: boolean }
  | { type: 'set-field-patch'; value: Record<string, string> }
  | { type: 'set-name'; value: string }
  | { type: 'set-notify-type'; value: NotifyType }
  | { type: 'set-show-form'; value: boolean }
  | { type: 'set-to-input'; value: string }

type TypeLabels = Record<NotifyType, string>

const SENSITIVE_FIELDS = new Set(['password', 'bot_token', 'device_key'])
const DEFAULT_CHANNEL_FORM: ChannelFormState = {
  configFields: { url: '' },
  deleteChannelId: null,
  editingId: null,
  isEnabled: true,
  name: '',
  notifyType: 'webhook',
  showForm: false,
  toAddresses: [],
  toInput: ''
}

function isPlausibleEmail(value: string): boolean {
  const at = value.indexOf('@')
  if (at <= 0 || at === value.length - 1) {
    return false
  }
  return value.slice(at + 1).includes('.')
}

function parseConfigJson(raw: string): Record<string, unknown> {
  try {
    return JSON.parse(raw) as Record<string, unknown>
  } catch {
    return {}
  }
}

function flattenConfigFields(parsed: Record<string, unknown>): Record<string, string> {
  const flat: Record<string, string> = {}
  for (const [key, value] of Object.entries(parsed)) {
    if (typeof value === 'string') {
      flat[key] = value
    } else if (typeof value === 'boolean' || typeof value === 'number') {
      flat[key] = String(value)
    }
  }
  return flat
}

function parseEmailConfig(parsed: Record<string, unknown>): { from: string; to: string[] } {
  const from = typeof parsed.from === 'string' ? parsed.from : ''
  const to = Array.isArray(parsed.to)
    ? (parsed.to as unknown[]).filter((value): value is string => typeof value === 'string')
    : []
  return { from, to }
}

function defaultConfigFieldsForType(type: NotifyType): Record<string, string> {
  switch (type) {
    case 'webhook':
      return { url: '' }
    case 'telegram':
      return { bot_token: '', chat_id: '' }
    case 'bark':
      return { server_url: '', device_key: '' }
    case 'email':
      return { from: '' }
    case 'apns':
      return {
        key_id: '',
        team_id: '',
        private_key: '',
        bundle_id: 'com.serverbee.mobile',
        sandbox: 'true'
      }
    default:
      return {}
  }
}

function createEditState(notification: Notification): ChannelFormState {
  const parsed = parseConfigJson(notification.config_json)
  const baseState = {
    ...DEFAULT_CHANNEL_FORM,
    editingId: notification.id,
    isEnabled: notification.enabled,
    name: notification.name,
    notifyType: notification.notify_type as NotifyType,
    showForm: true
  }

  if (notification.notify_type === 'email') {
    const { from, to } = parseEmailConfig(parsed)
    return { ...baseState, configFields: { from }, toAddresses: to }
  }

  return { ...baseState, configFields: flattenConfigFields(parsed) }
}

function channelFormReducer(state: ChannelFormState, action: ChannelFormAction): ChannelFormState {
  switch (action.type) {
    case 'add-recipient':
      return { ...state, toAddresses: [...state.toAddresses, action.value], toInput: '' }
    case 'edit-channel':
      return createEditState(action.notification)
    case 'prepare-create':
      return { ...DEFAULT_CHANNEL_FORM, showForm: true }
    case 'remove-recipient':
      return { ...state, toAddresses: state.toAddresses.filter((address) => address !== action.value) }
    case 'reset-form':
      return DEFAULT_CHANNEL_FORM
    case 'set-config-fields':
      return { ...state, configFields: action.value }
    case 'set-delete-channel-id':
      return { ...state, deleteChannelId: action.value }
    case 'set-enabled':
      return { ...state, isEnabled: action.value }
    case 'set-field-patch':
      return { ...state, configFields: { ...state.configFields, ...action.value } }
    case 'set-name':
      return { ...state, name: action.value }
    case 'set-notify-type':
      return {
        ...state,
        configFields: defaultConfigFieldsForType(action.value),
        notifyType: action.value,
        toAddresses: action.value === 'email' ? [] : state.toAddresses,
        toInput: action.value === 'email' ? '' : state.toInput
      }
    case 'set-show-form':
      return action.value ? { ...state, showForm: true } : DEFAULT_CHANNEL_FORM
    case 'set-to-input':
      return { ...state, toInput: action.value }
    default:
      return state
  }
}

function getTypeLabels(t: (key: string) => string): TypeLabels {
  return {
    webhook: t('notifications.type_webhook'),
    telegram: t('notifications.type_telegram'),
    bark: t('notifications.type_bark'),
    email: t('notifications.type_email'),
    apns: t('notifications.type_apns')
  }
}

export function NotificationChannelsSection({
  isLoading,
  notifications
}: {
  isLoading: boolean
  notifications: Notification[] | undefined
}) {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [state, dispatch] = useReducer(channelFormReducer, DEFAULT_CHANNEL_FORM)
  const apnsFileInputRef = useRef<HTMLInputElement>(null)
  const typeLabels = getTypeLabels(t)

  const createMutation = useMutation({
    mutationFn: (input: {
      config_json: Record<string, string | string[]>
      enabled: boolean
      name: string
      notify_type: string
    }) => api.post<Notification>('/api/notifications', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notifications'] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['notification-groups'] }).catch(() => undefined)
      dispatch({ type: 'reset-form' })
      toast.success(t('notifications.toast_created'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('notifications.create_failed'))
    }
  })
  const updateMutation = useMutation({
    mutationFn: ({
      id,
      patch
    }: {
      id: string
      patch: {
        config_json?: Record<string, string | string[]>
        enabled?: boolean
        name?: string
        notify_type?: string
      }
    }) => api.put<Notification>(`/api/notifications/${id}`, patch),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notifications'] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['notification-groups'] }).catch(() => undefined)
      toast.success(t('notifications.toast_channel_updated'))
      dispatch({ type: 'reset-form' })
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('notifications.channel_update_failed'))
    }
  })
  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/notifications/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notifications'] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['notification-groups'] }).catch(() => undefined)
      toast.success(t('notifications.toast_deleted'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('notifications.delete_failed'))
    }
  })
  const testMutation = useMutation({
    mutationFn: (id: string) => api.post(`/api/notifications/${id}/test`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notifications'] }).catch(() => undefined)
      toast.success(t('notifications.toast_test_sent'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('notifications.test_failed'), { duration: 8000 })
    }
  })

  const handleApnsFileUpload = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    if (!file) {
      return
    }
    const reader = new FileReader()
    reader.onload = (readerEvent) => {
      const content = readerEvent.target?.result
      if (typeof content === 'string') {
        dispatch({ type: 'set-field-patch', value: { private_key: content.trim() } })
      }
    }
    reader.readAsText(file)
    event.target.value = ''
  }

  const handleAddRecipient = () => {
    const trimmed = state.toInput.trim()
    if (trimmed === '' || state.toAddresses.includes(trimmed)) {
      return
    }
    if (!isPlausibleEmail(trimmed)) {
      toast.error(t('notifications.invalid_email', { address: trimmed }))
      return
    }
    dispatch({ type: 'add-recipient', value: trimmed })
  }

  const submitChannel = (payload: Record<string, string | string[]>) => {
    const basePayload = {
      name: state.name.trim(),
      notify_type: state.notifyType,
      config_json: payload,
      enabled: state.isEnabled
    }
    if (state.editingId) {
      updateMutation.mutate({ id: state.editingId, patch: basePayload })
    } else {
      createMutation.mutate(basePayload)
    }
  }

  const handleCreate = (event: FormEvent) => {
    event.preventDefault()
    if (state.name.trim().length === 0) {
      return
    }
    if (state.notifyType === 'email') {
      if (state.toAddresses.length === 0) {
        return
      }
      submitChannel(buildEmailPayload(state.configFields.from ?? '', state.toAddresses))
      return
    }
    submitChannel(state.configFields)
  }

  return (
    <div className="space-y-4 rounded-lg border bg-card p-6">
      <ChannelSectionHeader dispatch={dispatch} state={state} />
      <NotificationChannelDialog
        apnsFileInputRef={apnsFileInputRef}
        dispatch={dispatch}
        onAddRecipient={handleAddRecipient}
        onApnsFileUpload={handleApnsFileUpload}
        onRemoveRecipient={(address) => dispatch({ type: 'remove-recipient', value: address })}
        onSubmit={handleCreate}
        pending={createMutation.isPending || updateMutation.isPending}
        state={state}
        typeLabels={typeLabels}
      />
      <NotificationChannelList
        deleteChannelId={state.deleteChannelId}
        deletePending={deleteMutation.isPending}
        isLoading={isLoading}
        notifications={notifications}
        onDeleteClose={() => dispatch({ type: 'set-delete-channel-id', value: null })}
        onDeleteConfirm={(id) => {
          deleteMutation.mutate(id)
          dispatch({ type: 'set-delete-channel-id', value: null })
        }}
        onDeleteOpen={(id) => dispatch({ type: 'set-delete-channel-id', value: id })}
        onEdit={(notification) => dispatch({ notification, type: 'edit-channel' })}
        onTest={(id) => testMutation.mutate(id)}
        testPending={testMutation.isPending}
        typeLabels={typeLabels}
      />
    </div>
  )
}

function ChannelSectionHeader({ dispatch, state }: { dispatch: Dispatch<ChannelFormAction>; state: ChannelFormState }) {
  const { t } = useTranslation(['settings', 'common'])

  return (
    <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <h2 className="font-semibold text-lg">{t('notifications.channels')}</h2>
      <Dialog onOpenChange={(open) => dispatch({ type: 'set-show-form', value: open })} open={state.showForm}>
        <DialogTrigger
          onClick={() => dispatch({ type: 'prepare-create' })}
          render={<Button size="sm" variant="outline" />}
        >
          <Plus className="size-4" />
          {t('common:add')}
        </DialogTrigger>
      </Dialog>
    </div>
  )
}

function NotificationChannelDialog({
  apnsFileInputRef,
  dispatch,
  onAddRecipient,
  onApnsFileUpload,
  onRemoveRecipient,
  onSubmit,
  pending,
  state,
  typeLabels
}: {
  apnsFileInputRef: RefObject<HTMLInputElement | null>
  dispatch: Dispatch<ChannelFormAction>
  onAddRecipient: () => void
  onApnsFileUpload: (event: ChangeEvent<HTMLInputElement>) => void
  onRemoveRecipient: (address: string) => void
  onSubmit: (event: FormEvent) => void
  pending: boolean
  state: ChannelFormState
  typeLabels: TypeLabels
}) {
  const { t } = useTranslation(['settings', 'common'])

  return (
    <Dialog onOpenChange={(open) => dispatch({ type: 'set-show-form', value: open })} open={state.showForm}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {state.editingId ? t('notifications.edit_channel_title') : t('notifications.add_channel_title')}
          </DialogTitle>
          <DialogDescription>{t('notifications.channel_dialog_description')}</DialogDescription>
        </DialogHeader>
        <DialogBody>
          <NotificationChannelForm
            apnsFileInputRef={apnsFileInputRef}
            dispatch={dispatch}
            onAddRecipient={onAddRecipient}
            onApnsFileUpload={onApnsFileUpload}
            onRemoveRecipient={onRemoveRecipient}
            onSubmit={onSubmit}
            state={state}
            typeLabels={typeLabels}
          />
        </DialogBody>
        <DialogFooter>
          <Button onClick={() => dispatch({ type: 'reset-form' })} size="sm" type="button" variant="ghost">
            {t('common:cancel')}
          </Button>
          <Button disabled={pending} form="notification-channel-form" size="sm" type="submit">
            {state.editingId ? t('notifications.update_channel') : t('common:create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function NotificationChannelForm({
  apnsFileInputRef,
  dispatch,
  onAddRecipient,
  onApnsFileUpload,
  onRemoveRecipient,
  onSubmit,
  state,
  typeLabels
}: {
  apnsFileInputRef: RefObject<HTMLInputElement | null>
  dispatch: Dispatch<ChannelFormAction>
  onAddRecipient: () => void
  onApnsFileUpload: (event: ChangeEvent<HTMLInputElement>) => void
  onRemoveRecipient: (address: string) => void
  onSubmit: (event: FormEvent) => void
  state: ChannelFormState
  typeLabels: TypeLabels
}) {
  const { t } = useTranslation(['settings', 'common'])
  const configFieldLabels: Record<string, Record<string, string>> = {
    webhook: { url: t('notifications.webhook_url') },
    telegram: { bot_token: t('notifications.bot_token'), chat_id: t('notifications.chat_id') },
    bark: { server_url: t('notifications.bark_server'), device_key: t('notifications.bark_device_key') },
    email: { from: t('notifications.from_address') }
  }

  return (
    <form className="space-y-3" id="notification-channel-form" onSubmit={onSubmit}>
      <Input
        onChange={(event) => dispatch({ type: 'set-name', value: event.target.value })}
        placeholder={t('notifications.channel_name')}
        required
        type="text"
        value={state.name}
      />
      <Select
        disabled={state.editingId !== null}
        items={typeLabels}
        onValueChange={(value) => dispatch({ type: 'set-notify-type', value: value as NotifyType })}
        value={state.notifyType}
      >
        <SelectTrigger className="h-9 w-full">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {Object.entries(typeLabels).map(([value, label]) => (
            <SelectItem key={value} value={value}>
              {label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {state.editingId !== null && (
        <p className="text-muted-foreground text-xs">{t('notifications.type_locked_in_edit')}</p>
      )}
      {state.notifyType === 'apns' && (
        <ApnsFormFields
          apnsFileInputRef={apnsFileInputRef}
          configFields={state.configFields}
          onFieldChange={(patch) => dispatch({ type: 'set-field-patch', value: patch })}
          onFileUpload={onApnsFileUpload}
        />
      )}
      {state.notifyType === 'email' && (
        <EmailFormFields
          from={state.configFields.from ?? ''}
          onAddRecipient={onAddRecipient}
          onFromChange={(value) => dispatch({ type: 'set-field-patch', value: { from: value } })}
          onRemoveRecipient={onRemoveRecipient}
          onToInputChange={(value) => dispatch({ type: 'set-to-input', value })}
          toAddresses={state.toAddresses}
          toInput={state.toInput}
        />
      )}
      {state.notifyType !== 'apns' &&
        state.notifyType !== 'email' &&
        Object.entries(configFieldLabels[state.notifyType] ?? {}).map(([key, label]) => (
          <Input
            key={key}
            onChange={(event) => dispatch({ type: 'set-field-patch', value: { [key]: event.target.value } })}
            placeholder={label}
            required
            type={SENSITIVE_FIELDS.has(key) ? 'password' : 'text'}
            value={state.configFields[key] ?? ''}
          />
        ))}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <Label className="text-sm">{t('notifications.enabled_label')}</Label>
        <Switch checked={state.isEnabled} onCheckedChange={(value) => dispatch({ type: 'set-enabled', value })} />
      </div>
    </form>
  )
}

function NotificationChannelList({
  deleteChannelId,
  deletePending,
  isLoading,
  notifications,
  onDeleteClose,
  onDeleteConfirm,
  onDeleteOpen,
  onEdit,
  onTest,
  testPending,
  typeLabels
}: {
  deleteChannelId: string | null
  deletePending: boolean
  isLoading: boolean
  notifications: Notification[] | undefined
  onDeleteClose: () => void
  onDeleteConfirm: (id: string) => void
  onDeleteOpen: (id: string) => void
  onEdit: (notification: Notification) => void
  onTest: (id: string) => void
  testPending: boolean
  typeLabels: TypeLabels
}) {
  const { t } = useTranslation(['settings', 'common'])

  if (isLoading) {
    return (
      <div className="space-y-2">
        {Array.from({ length: 2 }, (_, i) => (
          <Skeleton className="h-12" key={`skel-${i.toString()}`} />
        ))}
      </div>
    )
  }

  if (!notifications || notifications.length === 0) {
    return <p className="text-center text-muted-foreground text-sm">{t('notifications.no_channels')}</p>
  }

  return (
    <div className="divide-y rounded-md border">
      {notifications.map((notification) => (
        <div
          className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
          key={notification.id}
        >
          <div className="flex items-center gap-3">
            <Bell className="size-4 text-muted-foreground" />
            <div>
              <p className="font-medium text-sm">{notification.name}</p>
              <p className="text-muted-foreground text-xs">
                {typeLabels[notification.notify_type as NotifyType] ?? notification.notify_type}
                {notification.enabled ? '' : ` ${t('notifications.disabled')}`}
              </p>
            </div>
          </div>
          <NotificationChannelActions
            deleteChannelId={deleteChannelId}
            deletePending={deletePending}
            notification={notification}
            onDeleteClose={onDeleteClose}
            onDeleteConfirm={onDeleteConfirm}
            onDeleteOpen={onDeleteOpen}
            onEdit={onEdit}
            onTest={onTest}
            testPending={testPending}
          />
        </div>
      ))}
    </div>
  )
}

function NotificationChannelActions({
  deleteChannelId,
  deletePending,
  notification,
  onDeleteClose,
  onDeleteConfirm,
  onDeleteOpen,
  onEdit,
  onTest,
  testPending
}: {
  deleteChannelId: string | null
  deletePending: boolean
  notification: Notification
  onDeleteClose: () => void
  onDeleteConfirm: (id: string) => void
  onDeleteOpen: (id: string) => void
  onEdit: (notification: Notification) => void
  onTest: (id: string) => void
  testPending: boolean
}) {
  const { t } = useTranslation(['settings', 'common'])

  return (
    <div className="flex gap-1">
      <Button
        aria-label={t('common:a11y.test_notification', { name: notification.name })}
        disabled={testPending}
        onClick={() => onTest(notification.id)}
        size="sm"
        variant="outline"
      >
        <Send className="size-3.5" />
      </Button>
      <Button
        aria-label={t('common:a11y.edit_notification', { name: notification.name })}
        onClick={() => onEdit(notification)}
        size="sm"
        variant="outline"
      >
        <Pencil className="size-3.5" />
      </Button>
      <AlertDialog onOpenChange={(open) => !open && onDeleteClose()} open={deleteChannelId === notification.id}>
        <AlertDialogTrigger
          onClick={() => onDeleteOpen(notification.id)}
          render={
            <Button
              aria-label={t('common:a11y.delete_notification', { name: notification.name })}
              disabled={deletePending}
              size="sm"
              variant="destructive"
            />
          }
        >
          <Trash2 className="size-3.5" />
        </AlertDialogTrigger>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('common:confirm_title')}</AlertDialogTitle>
            <AlertDialogDescription>{t('common:confirm_delete_message')}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
            <AlertDialogAction onClick={() => onDeleteConfirm(notification.id)} variant="destructive">
              {t('common:delete')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

export interface EmailFormFieldsProps {
  from: string
  onAddRecipient: () => void
  onFromChange: (value: string) => void
  onRemoveRecipient: (address: string) => void
  onToInputChange: (value: string) => void
  toAddresses: string[]
  toInput: string
}

export function EmailFormFields({
  from,
  onFromChange,
  toAddresses,
  toInput,
  onToInputChange,
  onAddRecipient,
  onRemoveRecipient
}: EmailFormFieldsProps) {
  const { t } = useTranslation(['settings', 'common'])
  return (
    <>
      <p className="text-muted-foreground text-xs">{t('notifications.email_help_text')}</p>
      <Input
        onChange={(e) => onFromChange(e.target.value)}
        placeholder={t('notifications.from_address')}
        required
        type="email"
        value={from}
      />
      <div className="space-y-2">
        <Label className="text-sm">{t('notifications.recipients_label')}</Label>
        <div className="flex gap-2">
          <Input
            onChange={(e) => onToInputChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault()
                onAddRecipient()
              }
            }}
            placeholder={t('notifications.recipient_placeholder')}
            type="email"
            value={toInput}
          />
          <Button onClick={onAddRecipient} size="sm" type="button">
            {t('notifications.add_recipient')}
          </Button>
        </div>
        {toAddresses.length > 0 && (
          <div className="flex flex-wrap gap-1">
            {toAddresses.map((address) => (
              <span className="inline-flex items-center gap-1 rounded-md bg-muted px-2 py-1 text-xs" key={address}>
                {address}
                <button
                  aria-label={t('notifications.remove_recipient_aria', { address })}
                  className="text-muted-foreground hover:text-foreground"
                  onClick={() => onRemoveRecipient(address)}
                  type="button"
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        )}
      </div>
    </>
  )
}

interface ApnsFormFieldsProps {
  apnsFileInputRef: RefObject<HTMLInputElement | null>
  configFields: Record<string, string>
  onFieldChange: (patch: Record<string, string>) => void
  onFileUpload: (event: ChangeEvent<HTMLInputElement>) => void
}

function ApnsFormFields({ apnsFileInputRef, configFields, onFieldChange, onFileUpload }: ApnsFormFieldsProps) {
  const { t } = useTranslation(['settings', 'common'])
  return (
    <>
      <Input
        maxLength={10}
        onChange={(e) => onFieldChange({ key_id: e.target.value })}
        placeholder={t('notifications.apns_key_id')}
        required
        type="text"
        value={configFields.key_id ?? ''}
      />
      <Input
        onChange={(e) => onFieldChange({ team_id: e.target.value })}
        placeholder={t('notifications.apns_team_id')}
        required
        type="text"
        value={configFields.team_id ?? ''}
      />
      <div className="space-y-1">
        <textarea
          aria-label={t('notifications.apns_private_key')}
          className="flex min-h-[80px] w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
          onChange={(e) => onFieldChange({ private_key: e.target.value })}
          placeholder={t('notifications.apns_private_key')}
          required
          rows={4}
          value={configFields.private_key ?? ''}
        />
        <input
          accept=".p8,.pem,.key,text/plain"
          aria-label={t('notifications.upload_p8_file')}
          className="hidden"
          onChange={onFileUpload}
          ref={apnsFileInputRef}
          type="file"
        />
        <Button
          className="h-7 text-xs"
          onClick={() => apnsFileInputRef.current?.click()}
          size="sm"
          type="button"
          variant="outline"
        >
          <Upload className="size-3" />
          {t('notifications.upload_p8_file')}
        </Button>
      </div>
      <Input
        onChange={(e) => onFieldChange({ bundle_id: e.target.value })}
        placeholder={t('notifications.apns_bundle_id')}
        required
        type="text"
        value={configFields.bundle_id ?? ''}
      />
      <Label className="cursor-pointer">
        <Switch
          checked={configFields.sandbox === 'true'}
          onCheckedChange={(checked) => onFieldChange({ sandbox: checked ? 'true' : 'false' })}
        />
        {t('notifications.apns_sandbox')}
      </Label>
    </>
  )
}
