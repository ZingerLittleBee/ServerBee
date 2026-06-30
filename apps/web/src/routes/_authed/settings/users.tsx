import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Plus, Trash2, UserCog } from 'lucide-react'
import { type FormEvent, useReducer } from 'react'
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
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { useAuth } from '@/hooks/use-auth'
import { api } from '@/lib/api-client'
import type { UserResponse } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/users')({
  component: UsersPage
})

interface UsersPageState {
  deleteUserId: string | null
  editingId: string | null
  editRole: string
  newPassword: string
  newRole: string
  newUsername: string
  showForm: boolean
}

type UsersPageAction =
  | { type: 'closeCreateForm' }
  | { type: 'setDeleteUserId'; value: string | null }
  | { type: 'setEditRole'; value: string }
  | { type: 'setNewPassword'; value: string }
  | { type: 'setNewRole'; value: string }
  | { type: 'setNewUsername'; value: string }
  | { type: 'setShowForm'; value: boolean }
  | { type: 'startEditing'; id: string; role: string }
  | { type: 'stopEditing' }

const INITIAL_USERS_PAGE_STATE: UsersPageState = {
  deleteUserId: null,
  editingId: null,
  editRole: '',
  newPassword: '',
  newRole: 'member',
  newUsername: '',
  showForm: false
}

function usersPageReducer(state: UsersPageState, action: UsersPageAction): UsersPageState {
  switch (action.type) {
    case 'closeCreateForm':
      return { ...state, newPassword: '', newRole: 'member', newUsername: '', showForm: false }
    case 'setDeleteUserId':
      return { ...state, deleteUserId: action.value }
    case 'setEditRole':
      return { ...state, editRole: action.value }
    case 'setNewPassword':
      return { ...state, newPassword: action.value }
    case 'setNewRole':
      return { ...state, newRole: action.value }
    case 'setNewUsername':
      return { ...state, newUsername: action.value }
    case 'setShowForm':
      return action.value ? { ...state, showForm: true } : usersPageReducer(state, { type: 'closeCreateForm' })
    case 'startEditing':
      return { ...state, editingId: action.id, editRole: action.role }
    case 'stopEditing':
      return { ...state, editingId: null }
    default:
      return state
  }
}

function UsersPage() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const { user: currentUser } = useAuth()
  const [state, dispatch] = useReducer(usersPageReducer, INITIAL_USERS_PAGE_STATE)

  const { data: users, isLoading } = useQuery<UserResponse[]>({
    queryKey: ['users'],
    queryFn: () => api.get<UserResponse[]>('/api/users')
  })

  const createMutation = useMutation({
    mutationFn: (input: { password: string; role: string; username: string }) =>
      api.post<UserResponse>('/api/users', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['users'] }).catch(() => undefined)
      dispatch({ type: 'closeCreateForm' })
      toast.success(t('users.toast_created'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
    }
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, role }: { id: string; role: string }) => api.put<UserResponse>(`/api/users/${id}`, { role }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['users'] }).catch(() => undefined)
      dispatch({ type: 'stopEditing' })
      toast.success(t('users.toast_role_updated'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
    }
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/users/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['users'] }).catch(() => undefined)
      toast.success(t('users.toast_deleted'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
    }
  })

  const handleCreate = (e: FormEvent) => {
    e.preventDefault()
    if (state.newUsername.trim().length === 0 || state.newPassword.length === 0) {
      return
    }
    createMutation.mutate({
      username: state.newUsername.trim(),
      password: state.newPassword,
      role: state.newRole
    })
  }

  return (
    <div className="max-w-2xl space-y-4">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h2 className="font-semibold text-lg">{t('users.count')}</h2>
        <Dialog onOpenChange={(open) => dispatch({ type: 'setShowForm', value: open })} open={state.showForm}>
          <DialogTrigger render={<Button size="sm" variant="outline" />}>
            <Plus className="size-4" />
            {t('users.add')}
          </DialogTrigger>
          <DialogContent className="sm:max-w-md">
            <DialogHeader>
              <DialogTitle>{t('users.add')}</DialogTitle>
              <DialogDescription>{t('users.add_description')}</DialogDescription>
            </DialogHeader>
            <form className="space-y-3" id="create-user-form" onSubmit={handleCreate}>
              <Input
                aria-label={t('users.username')}
                autoComplete="username"
                name="username"
                onChange={(e) => dispatch({ type: 'setNewUsername', value: e.target.value })}
                placeholder={t('users.username')}
                required
                spellCheck={false}
                type="text"
                value={state.newUsername}
              />
              <Input
                aria-label={t('users.password_hint')}
                autoComplete="new-password"
                minLength={6}
                name="password"
                onChange={(e) => dispatch({ type: 'setNewPassword', value: e.target.value })}
                placeholder={t('users.password_hint')}
                required
                type="password"
                value={state.newPassword}
              />
              <Select
                items={{ member: t('users.role_member'), admin: t('users.role_admin') }}
                onValueChange={(value) => value !== null && dispatch({ type: 'setNewRole', value })}
                value={state.newRole}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="member">{t('users.role_member')}</SelectItem>
                  <SelectItem value="admin">{t('users.role_admin')}</SelectItem>
                </SelectContent>
              </Select>
              {createMutation.error && <p className="text-destructive text-sm">{createMutation.error.message}</p>}
            </form>
            <DialogFooter>
              <Button onClick={() => dispatch({ type: 'closeCreateForm' })} size="sm" type="button" variant="ghost">
                {t('common:cancel')}
              </Button>
              <Button disabled={createMutation.isPending} form="create-user-form" size="sm" type="submit">
                {t('common:create')}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>

      {isLoading && (
        <div className="space-y-2">
          {Array.from({ length: 3 }, (_, i) => (
            <Skeleton className="h-12" key={`skel-${i.toString()}`} />
          ))}
        </div>
      )}
      {!isLoading && (!users || users.length === 0) && (
        <p className="text-center text-muted-foreground text-sm">{t('users.no_users')}</p>
      )}
      {users && users.length > 0 && (
        <div className="divide-y rounded-md border">
          {users.map((user) => (
            <div className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between" key={user.id}>
              <div className="flex min-w-0 items-center gap-3">
                <UserCog aria-hidden="true" className="size-4 text-muted-foreground" />
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
                    {state.editingId === user.id ? (
                      <span className="inline-flex items-center gap-2">
                        <Select
                          items={{ member: t('users.role_member'), admin: t('users.role_admin') }}
                          onValueChange={(value) => value !== null && dispatch({ type: 'setEditRole', value })}
                          value={state.editRole}
                        >
                          <SelectTrigger aria-label={t('users.role_label')} className="h-6 text-xs" size="sm">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="member">{t('users.role_member')}</SelectItem>
                            <SelectItem value="admin">{t('users.role_admin')}</SelectItem>
                          </SelectContent>
                        </Select>
                        <button
                          className="rounded text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                          onClick={() => updateMutation.mutate({ id: user.id, role: state.editRole })}
                          type="button"
                        >
                          {t('common:save')}
                        </button>
                        <button
                          className="rounded text-muted-foreground hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                          onClick={() => dispatch({ type: 'stopEditing' })}
                          type="button"
                        >
                          {t('common:cancel')}
                        </button>
                      </span>
                    ) : (
                      <span>
                        {t('users.role_label')}{' '}
                        <button
                          className="rounded font-medium hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                          onClick={() => dispatch({ type: 'startEditing', id: user.id, role: user.role })}
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
              {currentUser?.user_id !== user.id && (
                <AlertDialog
                  onOpenChange={(open) => {
                    if (!open) {
                      dispatch({ type: 'setDeleteUserId', value: null })
                    }
                  }}
                  open={state.deleteUserId === user.id}
                >
                  <AlertDialogTrigger
                    onClick={() => dispatch({ type: 'setDeleteUserId', value: user.id })}
                    render={
                      <Button
                        aria-label={`${t('users.delete')} ${user.username}`}
                        disabled={deleteMutation.isPending}
                        size="sm"
                        variant="destructive"
                      />
                    }
                  >
                    <Trash2 aria-hidden="true" className="size-3.5" />
                  </AlertDialogTrigger>
                  <AlertDialogContent>
                    <AlertDialogHeader>
                      <AlertDialogTitle>{t('common:confirm_title')}</AlertDialogTitle>
                      <AlertDialogDescription>{t('common:confirm_delete_message')}</AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                      <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
                      <AlertDialogAction
                        onClick={() => {
                          deleteMutation.mutate(user.id)
                          dispatch({ type: 'setDeleteUserId', value: null })
                        }}
                        variant="destructive"
                      >
                        {t('common:delete')}
                      </AlertDialogAction>
                    </AlertDialogFooter>
                  </AlertDialogContent>
                </AlertDialog>
              )}
            </div>
          ))}
        </div>
      )}
      {deleteMutation.error && <p className="text-destructive text-sm">{deleteMutation.error.message}</p>}
    </div>
  )
}
