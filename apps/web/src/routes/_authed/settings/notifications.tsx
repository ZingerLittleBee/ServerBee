import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Bell, Plus, Send, Trash2 } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { Notification, NotificationGroup } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/notifications')({
  component: NotificationsPage
})

type NotifyType = 'bark' | 'email' | 'telegram' | 'webhook'

const SENSITIVE_FIELDS = new Set(['password', 'bot_token', 'device_key'])

function NotificationsPage() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [showForm, setShowForm] = useState(false)
  const [name, setName] = useState('')
  const [notifyType, setNotifyType] = useState<NotifyType>('webhook')
  const [configFields, setConfigFields] = useState<Record<string, string>>({
    url: ''
  })

  const typeLabels: Record<NotifyType, string> = {
    webhook: t('notifications.type_webhook'),
    telegram: t('notifications.type_telegram'),
    bark: t('notifications.type_bark'),
    email: t('notifications.type_email')
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
    mutationFn: (input: { config_json: Record<string, string>; name: string; notify_type: string }) =>
      api.post<Notification>('/api/notifications', input),
    onSuccess: () => {
      invalidate()
      resetForm()
      toast.success('Notification channel created')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to create notification channel')
    }
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/notifications/${id}`),
    onSuccess: () => {
      invalidate()
      toast.success('Notification channel deleted')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to delete notification channel')
    }
  })

  const testMutation = useMutation({
    mutationFn: (id: string) => api.post(`/api/notifications/${id}/test`),
    onSuccess: () => {
      toast.success('Test notification sent')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to send test notification')
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
      toast.success('Notification group created')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to create notification group')
    }
  })

  const deleteGroupMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/notification-groups/${id}`),
    onSuccess: () => {
      invalidate()
      toast.success('Notification group deleted')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to delete notification group')
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
        setConfigFields({ smtp_host: '', smtp_port: '587', username: '', password: '', from: '', to: '' })
        break
      default:
        setConfigFields({})
    }
  }

  const handleCreate = (e: FormEvent) => {
    e.preventDefault()
    if (name.trim().length === 0) {
      return
    }
    createMutation.mutate({
      name: name.trim(),
      notify_type: notifyType,
      config_json: configFields
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
      smtp_host: t('notifications.smtp_host'),
      smtp_port: t('notifications.smtp_port'),
      username: t('notifications.smtp_username'),
      password: t('notifications.smtp_password'),
      from: t('notifications.from_address'),
      to: t('notifications.to_address')
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
              <Select onValueChange={(val) => handleTypeChange(val as NotifyType)} value={notifyType}>
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
              {Object.entries(configFieldLabels[notifyType] ?? {}).map(([key, label]) => (
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
                      aria-label={`Test ${n.name}`}
                      disabled={testMutation.isPending}
                      onClick={() => testMutation.mutate(n.id)}
                      size="sm"
                      variant="outline"
                    >
                      <Send className="size-3.5" />
                    </Button>
                    <Button
                      aria-label={`Delete ${n.name}`}
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
                      aria-label={`Delete group ${g.name}`}
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
