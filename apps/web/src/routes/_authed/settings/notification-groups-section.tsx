import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Pencil, Plus, Trash2 } from 'lucide-react'
import { type Dispatch, type FormEvent, useReducer } from 'react'
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
import { Checkbox } from '@/components/ui/checkbox'
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
import { api } from '@/lib/api-client'
import type { Notification, NotificationGroup } from '@/lib/api-schema'

type NotifyType = 'apns' | 'bark' | 'email' | 'telegram' | 'webhook'

interface GroupFormState {
  deleteGroupId: string | null
  editingGroupId: string | null
  groupName: string
  selectedIds: string[]
  showGroupForm: boolean
}

type GroupFormAction =
  | { group: NotificationGroup; type: 'edit-group' }
  | { type: 'prepare-create' }
  | { type: 'reset-form' }
  | { type: 'set-delete-group-id'; value: string | null }
  | { type: 'set-group-name'; value: string }
  | { type: 'set-show-group-form'; value: boolean }
  | { checked: boolean; notificationId: string; type: 'toggle-notification' }

const DEFAULT_GROUP_FORM: GroupFormState = {
  deleteGroupId: null,
  editingGroupId: null,
  groupName: '',
  selectedIds: [],
  showGroupForm: false
}

function parseGroupIds(raw: string | null | undefined): string[] {
  try {
    const parsed = JSON.parse(raw || '[]') as unknown
    if (Array.isArray(parsed)) {
      return parsed.filter((value): value is string => typeof value === 'string')
    }
  } catch {
    // fall through
  }
  return []
}

function groupFormReducer(state: GroupFormState, action: GroupFormAction): GroupFormState {
  switch (action.type) {
    case 'edit-group':
      return {
        deleteGroupId: null,
        editingGroupId: action.group.id,
        groupName: action.group.name,
        selectedIds: parseGroupIds(action.group.notification_ids_json),
        showGroupForm: true
      }
    case 'prepare-create':
      return { ...DEFAULT_GROUP_FORM, showGroupForm: true }
    case 'reset-form':
      return DEFAULT_GROUP_FORM
    case 'set-delete-group-id':
      return { ...state, deleteGroupId: action.value }
    case 'set-group-name':
      return { ...state, groupName: action.value }
    case 'set-show-group-form':
      return action.value ? { ...state, showGroupForm: true } : DEFAULT_GROUP_FORM
    case 'toggle-notification':
      return {
        ...state,
        selectedIds: action.checked
          ? [...state.selectedIds, action.notificationId]
          : state.selectedIds.filter((id) => id !== action.notificationId)
      }
    default:
      return state
  }
}

function getTypeLabels(t: (key: string) => string): Record<NotifyType, string> {
  return {
    webhook: t('notifications.type_webhook'),
    telegram: t('notifications.type_telegram'),
    bark: t('notifications.type_bark'),
    email: t('notifications.type_email'),
    apns: t('notifications.type_apns')
  }
}

export function NotificationGroupsSection({
  groups,
  notifications
}: {
  groups: NotificationGroup[] | undefined
  notifications: Notification[] | undefined
}) {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [state, dispatch] = useReducer(groupFormReducer, DEFAULT_GROUP_FORM)
  const typeLabels = getTypeLabels(t)

  const createGroupMutation = useMutation({
    mutationFn: (input: { name: string; notification_ids: string[] }) =>
      api.post<NotificationGroup>('/api/notification-groups', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notifications'] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['notification-groups'] }).catch(() => undefined)
      dispatch({ type: 'reset-form' })
      toast.success(t('notifications.toast_group_created'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('notifications.group_create_failed'))
    }
  })
  const updateGroupMutation = useMutation({
    mutationFn: ({ id, patch }: { id: string; patch: { name?: string; notification_ids?: string[] } }) =>
      api.put<NotificationGroup>(`/api/notification-groups/${id}`, patch),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notification-groups'] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['notifications'] }).catch(() => undefined)
      toast.success(t('notifications.toast_group_updated'))
      dispatch({ type: 'reset-form' })
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('notifications.group_update_failed'))
    }
  })
  const deleteGroupMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/notification-groups/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notifications'] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['notification-groups'] }).catch(() => undefined)
      toast.success(t('notifications.toast_group_deleted'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('notifications.group_delete_failed'))
    }
  })

  const handleCreateGroup = (event: FormEvent) => {
    event.preventDefault()
    if (state.groupName.trim().length === 0 || state.selectedIds.length === 0) {
      return
    }
    const body = { name: state.groupName.trim(), notification_ids: state.selectedIds }
    if (state.editingGroupId) {
      updateGroupMutation.mutate({ id: state.editingGroupId, patch: body })
    } else {
      createGroupMutation.mutate(body)
    }
  }

  return (
    <div className="space-y-4 rounded-lg border bg-card p-6">
      <GroupSectionHeader canCreate={!!notifications && notifications.length > 0} dispatch={dispatch} state={state} />
      <NotificationGroupDialog
        dispatch={dispatch}
        notifications={notifications}
        onSubmit={handleCreateGroup}
        pending={createGroupMutation.isPending || updateGroupMutation.isPending}
        state={state}
        typeLabels={typeLabels}
      />
      <NotificationGroupList
        deleteGroupId={state.deleteGroupId}
        deletePending={deleteGroupMutation.isPending}
        groups={groups}
        onDeleteClose={() => dispatch({ type: 'set-delete-group-id', value: null })}
        onDeleteConfirm={(id) => {
          deleteGroupMutation.mutate(id)
          dispatch({ type: 'set-delete-group-id', value: null })
        }}
        onDeleteOpen={(id) => dispatch({ type: 'set-delete-group-id', value: id })}
        onEdit={(group) => dispatch({ group, type: 'edit-group' })}
      />
    </div>
  )
}

function GroupSectionHeader({
  canCreate,
  dispatch,
  state
}: {
  canCreate: boolean
  dispatch: Dispatch<GroupFormAction>
  state: GroupFormState
}) {
  const { t } = useTranslation(['settings', 'common'])

  return (
    <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <h2 className="font-semibold text-lg">{t('notifications.groups')}</h2>
      <Dialog
        onOpenChange={(open) => dispatch({ type: 'set-show-group-form', value: open })}
        open={state.showGroupForm}
      >
        <DialogTrigger
          disabled={!canCreate}
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

function NotificationGroupDialog({
  dispatch,
  notifications,
  onSubmit,
  pending,
  state,
  typeLabels
}: {
  dispatch: Dispatch<GroupFormAction>
  notifications: Notification[] | undefined
  onSubmit: (event: FormEvent) => void
  pending: boolean
  state: GroupFormState
  typeLabels: Record<NotifyType, string>
}) {
  const { t } = useTranslation(['settings', 'common'])

  return (
    <Dialog onOpenChange={(open) => dispatch({ type: 'set-show-group-form', value: open })} open={state.showGroupForm}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {state.editingGroupId ? t('notifications.edit_group_title') : t('notifications.add_group_title')}
          </DialogTitle>
          <DialogDescription>{t('notifications.group_dialog_description')}</DialogDescription>
        </DialogHeader>
        <DialogBody>
          <form className="space-y-3" id="notification-group-form" onSubmit={onSubmit}>
            <Input
              onChange={(event) => dispatch({ type: 'set-group-name', value: event.target.value })}
              placeholder={t('notifications.group_name')}
              required
              type="text"
              value={state.groupName}
            />
            <fieldset className="space-y-2">
              <legend className="text-sm">{t('notifications.select_channels')}</legend>
              <div className="space-y-1 rounded-md border p-2">
                {notifications?.map((notification) => (
                  // biome-ignore lint/a11y/noLabelWithoutControl: Checkbox renders as a labelable button element
                  <label className="flex items-center gap-2 text-sm" key={notification.id}>
                    <Checkbox
                      checked={state.selectedIds.includes(notification.id)}
                      onCheckedChange={(checked) =>
                        dispatch({
                          checked: checked === true,
                          notificationId: notification.id,
                          type: 'toggle-notification'
                        })
                      }
                    />
                    {notification.name} (
                    {typeLabels[notification.notify_type as NotifyType] ?? notification.notify_type})
                  </label>
                ))}
              </div>
            </fieldset>
          </form>
        </DialogBody>
        <DialogFooter>
          <Button onClick={() => dispatch({ type: 'reset-form' })} size="sm" type="button" variant="ghost">
            {t('common:cancel')}
          </Button>
          <Button
            disabled={pending || state.selectedIds.length === 0}
            form="notification-group-form"
            size="sm"
            type="submit"
          >
            {state.editingGroupId ? t('notifications.update_group') : t('notifications.create_group')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function NotificationGroupList({
  deleteGroupId,
  deletePending,
  groups,
  onDeleteClose,
  onDeleteConfirm,
  onDeleteOpen,
  onEdit
}: {
  deleteGroupId: string | null
  deletePending: boolean
  groups: NotificationGroup[] | undefined
  onDeleteClose: () => void
  onDeleteConfirm: (id: string) => void
  onDeleteOpen: (id: string) => void
  onEdit: (group: NotificationGroup) => void
}) {
  const { t } = useTranslation(['settings', 'common'])

  if (!groups || groups.length === 0) {
    return <p className="text-center text-muted-foreground text-sm">{t('notifications.no_groups')}</p>
  }

  return (
    <div className="divide-y rounded-md border">
      {groups.map((group) => {
        const ids = parseGroupIds(group.notification_ids_json)
        return (
          <div className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between" key={group.id}>
            <div>
              <p className="font-medium text-sm">{group.name}</p>
              <p className="text-muted-foreground text-xs">{t('notifications.channel_count', { count: ids.length })}</p>
            </div>
            <div className="flex gap-1">
              <Button
                aria-label={t('common:a11y.edit_group', { name: group.name })}
                onClick={() => onEdit(group)}
                size="sm"
                variant="outline"
              >
                <Pencil className="size-3.5" />
              </Button>
              <AlertDialog onOpenChange={(open) => !open && onDeleteClose()} open={deleteGroupId === group.id}>
                <AlertDialogTrigger
                  onClick={() => onDeleteOpen(group.id)}
                  render={
                    <Button
                      aria-label={t('common:a11y.delete_group', { name: group.name })}
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
                    <AlertDialogAction onClick={() => onDeleteConfirm(group.id)} variant="destructive">
                      {t('common:delete')}
                    </AlertDialogAction>
                  </AlertDialogFooter>
                </AlertDialogContent>
              </AlertDialog>
            </div>
          </div>
        )
      })}
    </div>
  )
}
