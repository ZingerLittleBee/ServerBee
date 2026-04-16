import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Bell, Plus, Send, Trash2, Upload } from 'lucide-react'
import { type FormEvent, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { api } from '@/lib/api-client'
import type { Notification, NotificationGroup } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/notifications')({
  component: NotificationsPage
})

type NotifyType = 'apns' | 'bark' | 'email' | 'telegram' | 'webhook'

export function buildEmailPayload(from: string, toAddresses: string[]): { from: string; to: string[] } {
  return { from, to: toAddresses }
}

function isPlausibleEmail(s: string): boolean {
  const at = s.indexOf('@')
  if (at <= 0 || at === s.length - 1) {
    return false
  }
  const domain = s.slice(at + 1)
  return domain.includes('.')
}

const SENSITIVE_FIELDS = new Set(['password', 'bot_token', 'device_key'])

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
            {toAddresses.map((addr) => (
              <span className="inline-flex items-center gap-1 rounded-md bg-muted px-2 py-1 text-xs" key={addr}>
                {addr}
                <button
                  aria-label={t('notifications.remove_recipient_aria', { address: addr })}
                  className="text-muted-foreground hover:text-foreground"
                  onClick={() => onRemoveRecipient(addr)}
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

function NotificationsPage() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [showForm, setShowForm] = useState(false)
  const [name, setName] = useState('')
  const [notifyType, setNotifyType] = useState<NotifyType>('webhook')
  const [configFields, setConfigFields] = useState<Record<string, string>>({
    url: ''
  })
  const [toAddresses, setToAddresses] = useState<string[]>([])
  const [toInput, setToInput] = useState('')
  const apnsFileInputRef = useRef<HTMLInputElement>(null)

  const typeLabels: Record<NotifyType, string> = {
    webhook: t('notifications.type_webhook'),
    telegram: t('notifications.type_telegram'),
    bark: t('notifications.type_bark'),
    email: t('notifications.type_email'),
    apns: t('notifications.type_apns')
  }

  const { data: notifications, isLoading } = useQuery<Notification[]>({
    queryKey: ['notifications'],
    queryFn: () => api.get<Notification[]>('/api/notifications')
  })

  const { data: groups } = useQuery<NotificationGroup[]>({
    queryKey: ['notification-groups'],
    queryFn: () => api.get<NotificationGroup[]>('/api/notification-groups')
  })

  const createMutation = useMutation({
    mutationFn: (input: { config_json: Record<string, string | string[]>; name: string; notify_type: string }) =>
      api.post<Notification>('/api/notifications', input),
    onSuccess: () => {
      invalidate()
      resetForm()
      toast.success(t('notifications.toast_created'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('notifications.create_failed'))
    }
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/notifications/${id}`),
    onSuccess: () => {
      invalidate()
      toast.success(t('notifications.toast_deleted'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('notifications.delete_failed'))
    }
  })

  const testMutation = useMutation({
    mutationFn: (id: string) => api.post(`/api/notifications/${id}/test`),
    onSuccess: () => {
      toast.success(t('notifications.toast_test_sent'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('notifications.test_failed'), { duration: 8000 })
    }
  })

  // Groups
  const [groupName, setGroupName] = useState('')
  const [selectedIds, setSelectedIds] = useState<string[]>([])
  const [showGroupForm, setShowGroupForm] = useState(false)

  const createGroupMutation = useMutation({
    mutationFn: (input: { name: string; notification_ids: string[] }) =>
      api.post<NotificationGroup>('/api/notification-groups', input),
    onSuccess: () => {
      invalidate()
      setGroupName('')
      setSelectedIds([])
      setShowGroupForm(false)
      toast.success(t('notifications.toast_group_created'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('notifications.group_create_failed'))
    }
  })

  const deleteGroupMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/notification-groups/${id}`),
    onSuccess: () => {
      invalidate()
      toast.success(t('notifications.toast_group_deleted'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('notifications.group_delete_failed'))
    }
  })

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ['notifications'] }).catch(() => undefined)
    queryClient.invalidateQueries({ queryKey: ['notification-groups'] }).catch(() => undefined)
  }

  const resetForm = () => {
    setName('')
    setNotifyType('webhook')
    setConfigFields({ url: '' })
    setToAddresses([])
    setToInput('')
    setShowForm(false)
  }

  const handleTypeChange = (type: NotifyType) => {
    setNotifyType(type)
    switch (type) {
      case 'webhook':
        setConfigFields({ url: '' })
        break
      case 'telegram':
        setConfigFields({ bot_token: '', chat_id: '' })
        break
      case 'bark':
        setConfigFields({ server_url: '', device_key: '' })
        break
      case 'email':
        setConfigFields({ from: '' })
        setToAddresses([])
        setToInput('')
        break
      case 'apns':
        setConfigFields({
          key_id: '',
          team_id: '',
          private_key: '',
          bundle_id: 'com.serverbee.mobile',
          sandbox: 'true'
        })
        break
      default:
        setConfigFields({})
    }
  }

  const handleApnsFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) {
      return
    }
    const reader = new FileReader()
    reader.onload = (ev) => {
      const content = ev.target?.result
      if (typeof content === 'string') {
        setConfigFields((prev) => ({ ...prev, private_key: content.trim() }))
      }
    }
    reader.readAsText(file)
    // reset so the same file can be re-selected if needed
    e.target.value = ''
  }

  const handleAddRecipient = () => {
    const trimmed = toInput.trim()
    if (trimmed === '' || toAddresses.includes(trimmed)) {
      return
    }
    if (!isPlausibleEmail(trimmed)) {
      toast.error(t('notifications.invalid_email', { address: trimmed }))
      return
    }
    setToAddresses((prev) => [...prev, trimmed])
    setToInput('')
  }

  const handleRemoveRecipient = (addr: string) => {
    setToAddresses((prev) => prev.filter((a) => a !== addr))
  }

  const handleCreate = (e: FormEvent) => {
    e.preventDefault()
    if (name.trim().length === 0) {
      return
    }
    let payload: Record<string, string | string[]> = configFields
    if (notifyType === 'email') {
      if (toAddresses.length === 0) {
        return
      }
      payload = buildEmailPayload(configFields.from ?? '', toAddresses)
    }
    createMutation.mutate({
      name: name.trim(),
      notify_type: notifyType,
      config_json: payload
    })
  }

  const handleCreateGroup = (e: FormEvent) => {
    e.preventDefault()
    if (groupName.trim().length === 0 || selectedIds.length === 0) {
      return
    }
    createGroupMutation.mutate({
      name: groupName.trim(),
      notification_ids: selectedIds
    })
  }

  const configFieldLabels: Record<string, Record<string, string>> = {
    webhook: { url: t('notifications.webhook_url') },
    telegram: { bot_token: t('notifications.bot_token'), chat_id: t('notifications.chat_id') },
    bark: { server_url: t('notifications.bark_server'), device_key: t('notifications.bark_device_key') },
    email: {
      from: t('notifications.from_address')
    }
  }

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">{t('notifications.title')}</h1>

      <div className="max-w-2xl space-y-6">
        {/* Create notification */}
        <div className="rounded-lg border bg-card p-6">
          <div className="mb-4 flex items-center justify-between">
            <h2 className="font-semibold text-lg">{t('notifications.channels')}</h2>
            <Button onClick={() => setShowForm(!showForm)} size="sm" variant="outline">
              <Plus className="size-4" />
              {t('common:add')}
            </Button>
          </div>

          {showForm && (
            <form className="mb-4 space-y-3 rounded-md border bg-muted/30 p-4" onSubmit={handleCreate}>
              <Input
                onChange={(e) => setName(e.target.value)}
                placeholder={t('notifications.channel_name')}
                required
                type="text"
                value={name}
              />
              <Select
                items={typeLabels}
                onValueChange={(val) => handleTypeChange(val as NotifyType)}
                value={notifyType}
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
              {notifyType === 'apns' && (
                <>
                  <Input
                    maxLength={10}
                    onChange={(e) => setConfigFields((prev) => ({ ...prev, key_id: e.target.value }))}
                    placeholder={t('notifications.apns_key_id')}
                    required
                    type="text"
                    value={configFields.key_id ?? ''}
                  />
                  <Input
                    onChange={(e) => setConfigFields((prev) => ({ ...prev, team_id: e.target.value }))}
                    placeholder={t('notifications.apns_team_id')}
                    required
                    type="text"
                    value={configFields.team_id ?? ''}
                  />
                  <div className="space-y-1">
                    <textarea
                      className="flex min-h-[80px] w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
                      onChange={(e) => setConfigFields((prev) => ({ ...prev, private_key: e.target.value }))}
                      placeholder={t('notifications.apns_private_key')}
                      required
                      rows={4}
                      value={configFields.private_key ?? ''}
                    />
                    <input
                      accept=".p8,.pem,.key,text/plain"
                      className="hidden"
                      onChange={handleApnsFileUpload}
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
                    onChange={(e) => setConfigFields((prev) => ({ ...prev, bundle_id: e.target.value }))}
                    placeholder={t('notifications.apns_bundle_id')}
                    required
                    type="text"
                    value={configFields.bundle_id ?? ''}
                  />
                  <Label className="cursor-pointer">
                    <Switch
                      checked={configFields.sandbox === 'true'}
                      onCheckedChange={(checked) =>
                        setConfigFields((prev) => ({ ...prev, sandbox: checked ? 'true' : 'false' }))
                      }
                    />
                    {t('notifications.apns_sandbox')}
                  </Label>
                </>
              )}
              {notifyType === 'email' && (
                <EmailFormFields
                  from={configFields.from ?? ''}
                  onAddRecipient={handleAddRecipient}
                  onFromChange={(value) => setConfigFields((prev) => ({ ...prev, from: value }))}
                  onRemoveRecipient={handleRemoveRecipient}
                  onToInputChange={setToInput}
                  toAddresses={toAddresses}
                  toInput={toInput}
                />
              )}
              {notifyType !== 'apns' &&
                notifyType !== 'email' &&
                Object.entries(configFieldLabels[notifyType] ?? {}).map(([key, label]) => (
                  <Input
                    key={key}
                    onChange={(e) => setConfigFields((prev) => ({ ...prev, [key]: e.target.value }))}
                    placeholder={label}
                    required
                    type={SENSITIVE_FIELDS.has(key) ? 'password' : 'text'}
                    value={configFields[key] ?? ''}
                  />
                ))}
              <div className="flex gap-2">
                <Button disabled={createMutation.isPending} size="sm" type="submit">
                  {t('common:create')}
                </Button>
                <Button onClick={resetForm} size="sm" type="button" variant="ghost">
                  {t('common:cancel')}
                </Button>
              </div>
            </form>
          )}

          {isLoading && (
            <div className="space-y-2">
              {Array.from({ length: 2 }, (_, i) => (
                <Skeleton className="h-12" key={`skel-${i.toString()}`} />
              ))}
            </div>
          )}
          {!isLoading && (!notifications || notifications.length === 0) && (
            <p className="text-center text-muted-foreground text-sm">{t('notifications.no_channels')}</p>
          )}
          {notifications && notifications.length > 0 && (
            <div className="divide-y rounded-md border">
              {notifications.map((n) => (
                <div className="flex items-center justify-between px-4 py-3" key={n.id}>
                  <div className="flex items-center gap-3">
                    <Bell className="size-4 text-muted-foreground" />
                    <div>
                      <p className="font-medium text-sm">{n.name}</p>
                      <p className="text-muted-foreground text-xs">
                        {typeLabels[n.notify_type as NotifyType] ?? n.notify_type}
                        {n.enabled ? '' : ` ${t('notifications.disabled')}`}
                      </p>
                    </div>
                  </div>
                  <div className="flex gap-1">
                    <Button
                      aria-label={t('common:a11y.test_notification', { name: n.name })}
                      disabled={testMutation.isPending}
                      onClick={() => testMutation.mutate(n.id)}
                      size="sm"
                      variant="outline"
                    >
                      <Send className="size-3.5" />
                    </Button>
                    <Button
                      aria-label={t('common:a11y.delete_notification', { name: n.name })}
                      disabled={deleteMutation.isPending}
                      onClick={() => deleteMutation.mutate(n.id)}
                      size="sm"
                      variant="destructive"
                    >
                      <Trash2 className="size-3.5" />
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Notification Groups */}
        <div className="rounded-lg border bg-card p-6">
          <div className="mb-4 flex items-center justify-between">
            <h2 className="font-semibold text-lg">{t('notifications.groups')}</h2>
            <Button onClick={() => setShowGroupForm(!showGroupForm)} size="sm" variant="outline">
              <Plus className="size-4" />
              {t('common:add')}
            </Button>
          </div>

          {showGroupForm && notifications && notifications.length > 0 && (
            <form className="mb-4 space-y-3 rounded-md border bg-muted/30 p-4" onSubmit={handleCreateGroup}>
              <Input
                onChange={(e) => setGroupName(e.target.value)}
                placeholder={t('notifications.group_name')}
                required
                type="text"
                value={groupName}
              />
              <fieldset className="space-y-1">
                <legend className="text-sm">{t('notifications.select_channels')}</legend>
                {notifications.map((n) => (
                  // biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element
                  <label className="flex items-center gap-2 text-sm" key={n.id}>
                    <Checkbox
                      checked={selectedIds.includes(n.id)}
                      onCheckedChange={(checked) => {
                        setSelectedIds((prev) => (checked ? [...prev, n.id] : prev.filter((id) => id !== n.id)))
                      }}
                    />
                    {n.name} ({typeLabels[n.notify_type as NotifyType] ?? n.notify_type})
                  </label>
                ))}
              </fieldset>
              <Button disabled={createGroupMutation.isPending || selectedIds.length === 0} size="sm" type="submit">
                {t('notifications.create_group')}
              </Button>
            </form>
          )}

          {!groups || groups.length === 0 ? (
            <p className="text-center text-muted-foreground text-sm">{t('notifications.no_groups')}</p>
          ) : (
            <div className="divide-y rounded-md border">
              {groups.map((g) => {
                const ids: string[] = JSON.parse(g.notification_ids_json || '[]')
                return (
                  <div className="flex items-center justify-between px-4 py-3" key={g.id}>
                    <div>
                      <p className="font-medium text-sm">{g.name}</p>
                      <p className="text-muted-foreground text-xs">
                        {t('notifications.channel_count', { count: ids.length })}
                      </p>
                    </div>
                    <Button
                      aria-label={t('common:a11y.delete_group', { name: g.name })}
                      disabled={deleteGroupMutation.isPending}
                      onClick={() => deleteGroupMutation.mutate(g.id)}
                      size="sm"
                      variant="destructive"
                    >
                      <Trash2 className="size-3.5" />
                    </Button>
                  </div>
                )
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
