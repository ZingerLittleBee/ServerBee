import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Bell, Plus, Send, Trash2 } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api-client'
import type { Notification, NotificationGroup } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/notifications')({
  component: NotificationsPage
})

type NotifyType = 'bark' | 'email' | 'telegram' | 'webhook'

const typeLabels: Record<NotifyType, string> = {
  webhook: 'Webhook',
  telegram: 'Telegram',
  bark: 'Bark',
  email: 'Email'
}

function NotificationsPage() {
  const queryClient = useQueryClient()
  const [showForm, setShowForm] = useState(false)
  const [name, setName] = useState('')
  const [notifyType, setNotifyType] = useState<NotifyType>('webhook')
  const [configFields, setConfigFields] = useState<Record<string, string>>({
    url: ''
  })

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
    }
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/notifications/${id}`),
    onSuccess: () => invalidate()
  })

  const testMutation = useMutation({
    mutationFn: (id: string) => api.post(`/api/notifications/${id}/test`)
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
    }
  })

  const deleteGroupMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/notification-groups/${id}`),
    onSuccess: () => invalidate()
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
    webhook: { url: 'Webhook URL' },
    telegram: { bot_token: 'Bot Token', chat_id: 'Chat ID' },
    bark: { server_url: 'Server URL', device_key: 'Device Key' },
    email: {
      smtp_host: 'SMTP Host',
      smtp_port: 'SMTP Port',
      username: 'Username',
      password: 'Password',
      from: 'From Address',
      to: 'To Address'
    }
  }

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">Notifications</h1>

      <div className="max-w-2xl space-y-6">
        {/* Create notification */}
        <div className="rounded-lg border bg-card p-6">
          <div className="mb-4 flex items-center justify-between">
            <h2 className="font-semibold text-lg">Notification Channels</h2>
            <Button onClick={() => setShowForm(!showForm)} size="sm" variant="outline">
              <Plus className="size-4" />
              Add
            </Button>
          </div>

          {showForm && (
            <form className="mb-4 space-y-3 rounded-md border bg-muted/30 p-4" onSubmit={handleCreate}>
              <input
                className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                onChange={(e) => setName(e.target.value)}
                placeholder="Channel name"
                required
                type="text"
                value={name}
              />
              <select
                className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm"
                onChange={(e) => handleTypeChange(e.target.value as NotifyType)}
                value={notifyType}
              >
                {Object.entries(typeLabels).map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
              {Object.entries(configFieldLabels[notifyType] ?? {}).map(([key, label]) => (
                <input
                  className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                  key={key}
                  onChange={(e) => setConfigFields((prev) => ({ ...prev, [key]: e.target.value }))}
                  placeholder={label}
                  required
                  type="text"
                  value={configFields[key] ?? ''}
                />
              ))}
              <div className="flex gap-2">
                <Button disabled={createMutation.isPending} size="sm" type="submit">
                  Create
                </Button>
                <Button onClick={resetForm} size="sm" type="button" variant="ghost">
                  Cancel
                </Button>
              </div>
            </form>
          )}

          {isLoading && (
            <div className="space-y-2">
              {Array.from({ length: 2 }, (_, i) => (
                <div className="h-12 animate-pulse rounded bg-muted" key={`skel-${i.toString()}`} />
              ))}
            </div>
          )}
          {!isLoading && (!notifications || notifications.length === 0) && (
            <p className="text-center text-muted-foreground text-sm">No notification channels configured</p>
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
                        {n.enabled ? '' : ' (disabled)'}
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
            <h2 className="font-semibold text-lg">Notification Groups</h2>
            <Button onClick={() => setShowGroupForm(!showGroupForm)} size="sm" variant="outline">
              <Plus className="size-4" />
              Add
            </Button>
          </div>

          {showGroupForm && notifications && notifications.length > 0 && (
            <form className="mb-4 space-y-3 rounded-md border bg-muted/30 p-4" onSubmit={handleCreateGroup}>
              <input
                className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                onChange={(e) => setGroupName(e.target.value)}
                placeholder="Group name"
                required
                type="text"
                value={groupName}
              />
              <fieldset className="space-y-1">
                <legend className="text-sm">Select channels:</legend>
                {notifications.map((n) => (
                  <label className="flex items-center gap-2 text-sm" key={n.id}>
                    <input
                      checked={selectedIds.includes(n.id)}
                      onChange={(e) => {
                        setSelectedIds((prev) =>
                          e.target.checked ? [...prev, n.id] : prev.filter((id) => id !== n.id)
                        )
                      }}
                      type="checkbox"
                    />
                    {n.name} ({typeLabels[n.notify_type as NotifyType] ?? n.notify_type})
                  </label>
                ))}
              </fieldset>
              <Button disabled={createGroupMutation.isPending || selectedIds.length === 0} size="sm" type="submit">
                Create Group
              </Button>
            </form>
          )}

          {!groups || groups.length === 0 ? (
            <p className="text-center text-muted-foreground text-sm">No notification groups</p>
          ) : (
            <div className="divide-y rounded-md border">
              {groups.map((g) => {
                const ids: string[] = JSON.parse(g.notification_ids_json || '[]')
                return (
                  <div className="flex items-center justify-between px-4 py-3" key={g.id}>
                    <div>
                      <p className="font-medium text-sm">{g.name}</p>
                      <p className="text-muted-foreground text-xs">{ids.length} channel(s)</p>
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
