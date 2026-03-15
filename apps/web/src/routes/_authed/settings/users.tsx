import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Plus, Trash2, UserCog } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api-client'
import type { UserResponse } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/users')({
  component: UsersPage
})

function UsersPage() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [showForm, setShowForm] = useState(false)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [editRole, setEditRole] = useState('')
  const [newUsername, setNewUsername] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [newRole, setNewRole] = useState('member')

  const { data: users, isLoading } = useQuery<UserResponse[]>({
    queryKey: ['users'],
    queryFn: () => api.get<UserResponse[]>('/api/users')
  })

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ['users'] }).catch(() => undefined)
  }

  const createMutation = useMutation({
    mutationFn: (input: { password: string; role: string; username: string }) =>
      api.post<UserResponse>('/api/users', input),
    onSuccess: () => {
      invalidate()
      resetForm()
    }
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, role }: { id: string; role: string }) => api.put<UserResponse>(`/api/users/${id}`, { role }),
    onSuccess: () => {
      invalidate()
      setEditingId(null)
    }
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/users/${id}`),
    onSuccess: () => invalidate()
  })

  const resetForm = () => {
    setNewUsername('')
    setNewPassword('')
    setNewRole('member')
    setShowForm(false)
  }

  const handleCreate = (e: FormEvent) => {
    e.preventDefault()
    if (newUsername.trim().length === 0 || newPassword.length === 0) {
      return
    }
    createMutation.mutate({
      username: newUsername.trim(),
      password: newPassword,
      role: newRole
    })
  }

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">{t('users.title')}</h1>

      <div className="max-w-2xl space-y-6">
        <div className="rounded-lg border bg-card p-6">
          <div className="mb-4 flex items-center justify-between">
            <h2 className="font-semibold text-lg">{t('users.count')}</h2>
            <Button onClick={() => setShowForm(!showForm)} size="sm" variant="outline">
              <Plus className="size-4" />
              {t('users.add')}
            </Button>
          </div>

          {showForm && (
            <form className="mb-4 space-y-3 rounded-md border bg-muted/30 p-4" onSubmit={handleCreate}>
              <input
                className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                onChange={(e) => setNewUsername(e.target.value)}
                placeholder={t('users.username')}
                required
                type="text"
                value={newUsername}
              />
              <input
                className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                minLength={6}
                onChange={(e) => setNewPassword(e.target.value)}
                placeholder={t('users.password_hint')}
                required
                type="password"
                value={newPassword}
              />
              <select
                className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm"
                onChange={(e) => setNewRole(e.target.value)}
                value={newRole}
              >
                <option value="member">{t('users.role_member')}</option>
                <option value="admin">{t('users.role_admin')}</option>
              </select>
              <div className="flex gap-2">
                <Button disabled={createMutation.isPending} size="sm" type="submit">
                  {t('common:create')}
                </Button>
                <Button onClick={resetForm} size="sm" type="button" variant="ghost">
                  {t('common:cancel')}
                </Button>
              </div>
              {createMutation.error && <p className="text-destructive text-sm">{createMutation.error.message}</p>}
            </form>
          )}

          {isLoading && (
            <div className="space-y-2">
              {Array.from({ length: 3 }, (_, i) => (
                <div className="h-12 animate-pulse rounded bg-muted" key={`skel-${i.toString()}`} />
              ))}
            </div>
          )}
          {!isLoading && (!users || users.length === 0) && (
            <p className="text-center text-muted-foreground text-sm">{t('users.no_users')}</p>
          )}
          {users && users.length > 0 && (
            <div className="divide-y rounded-md border">
              {users.map((user) => (
                <div className="flex items-center justify-between px-4 py-3" key={user.id}>
                  <div className="flex items-center gap-3">
                    <UserCog className="size-4 text-muted-foreground" />
                    <div>
                      <p className="font-medium text-sm">
                        {user.username}
                        {user.has_2fa && (
                          <span className="ml-2 rounded bg-green-100 px-1.5 py-0.5 font-normal text-green-700 text-xs dark:bg-green-900/30 dark:text-green-400">
                            {t('users.two_factor')}
                          </span>
                        )}
                      </p>
                      <p className="text-muted-foreground text-xs">
                        {editingId === user.id ? (
                          <span className="inline-flex items-center gap-2">
                            <select
                              className="rounded border border-input bg-transparent px-1 py-0.5 text-xs"
                              onChange={(e) => setEditRole(e.target.value)}
                              value={editRole}
                            >
                              <option value="member">member</option>
                              <option value="admin">admin</option>
                            </select>
                            <button
                              className="text-primary hover:underline"
                              onClick={() => updateMutation.mutate({ id: user.id, role: editRole })}
                              type="button"
                            >
                              Save
                            </button>
                            <button
                              className="text-muted-foreground hover:underline"
                              onClick={() => setEditingId(null)}
                              type="button"
                            >
                              {t('common:cancel')}
                            </button>
                          </span>
                        ) : (
                          <span>
                            {t('users.role_label')}{' '}
                            <button
                              className="font-medium hover:underline"
                              onClick={() => {
                                setEditingId(user.id)
                                setEditRole(user.role)
                              }}
                              type="button"
                            >
                              {user.role}
                            </button>
                            {' · '}
                            {t('users.created')} {new Date(user.created_at).toLocaleDateString()}
                          </span>
                        )}
                      </p>
                    </div>
                  </div>
                  <Button
                    aria-label={`${t('users.delete')} ${user.username}`}
                    disabled={deleteMutation.isPending}
                    onClick={() => {
                      // biome-ignore lint/suspicious/noAlert: temporary UX, will replace with dialog
                      if (window.confirm(`Delete user "${user.username}"? This cannot be undone.`)) {
                        deleteMutation.mutate(user.id)
                      }
                    }}
                    size="sm"
                    variant="destructive"
                  >
                    <Trash2 className="size-3.5" />
                  </Button>
                </div>
              ))}
            </div>
          )}
          {deleteMutation.error && <p className="mt-2 text-destructive text-sm">{deleteMutation.error.message}</p>}
        </div>
      </div>
    </div>
  )
}
