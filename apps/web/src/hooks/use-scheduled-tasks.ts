import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { api } from '@/lib/api-client'

export interface ScheduledTask {
  command: string
  created_at: string
  cron_expression: string | null
  enabled: boolean
  id: string
  last_run_at: string | null
  name: string | null
  next_run_at: string | null
  retry_count: number
  retry_interval: number
  server_ids: string[]
  task_type: string
  timeout: number | null
}

export interface ScheduledTaskResult {
  attempt: number
  exit_code: number
  finished_at: string
  id: number
  output: string
  run_id: string | null
  server_id: string
  started_at: string | null
  task_id: string
}

export interface CreateScheduledTaskInput {
  command: string
  cron_expression: string
  name: string
  retry_count?: number
  retry_interval?: number
  server_ids: string[]
  timeout?: number
}

export interface UpdateScheduledTaskInput {
  command?: string
  cron_expression?: string
  enabled?: boolean
  name?: string
  retry_count?: number
  retry_interval?: number
  server_ids?: string[]
  timeout?: number
}

export function useScheduledTasks() {
  return useQuery<ScheduledTask[]>({
    queryKey: ['tasks', 'scheduled'],
    queryFn: () => api.get<ScheduledTask[]>('/api/tasks?type=scheduled'),
    staleTime: 30_000
  })
}

export function useScheduledTaskResults(taskId: string | null) {
  return useQuery<ScheduledTaskResult[]>({
    queryKey: ['tasks', taskId, 'results'],
    queryFn: () => api.get<ScheduledTaskResult[]>(`/api/tasks/${taskId}/results`),
    enabled: !!taskId,
    refetchInterval: taskId ? 5000 : false
  })
}

export function useCreateScheduledTask() {
  const queryClient = useQueryClient()
  const { t } = useTranslation('settings')
  return useMutation({
    mutationFn: (input: CreateScheduledTaskInput) => api.post('/api/tasks', { ...input, task_type: 'scheduled' }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tasks'] })
      toast.success(t('tasks.scheduled.toast_created', { defaultValue: 'Scheduled task created' }))
    }
  })
}

export function useUpdateScheduledTask() {
  const queryClient = useQueryClient()
  const { t } = useTranslation('settings')
  return useMutation({
    mutationFn: ({ id, ...input }: { id: string } & UpdateScheduledTaskInput) => api.put(`/api/tasks/${id}`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tasks'] })
      toast.success(t('tasks.scheduled.toast_updated', { defaultValue: 'Task updated' }))
    }
  })
}

export function useDeleteScheduledTask() {
  const queryClient = useQueryClient()
  const { t } = useTranslation('settings')
  return useMutation({
    mutationFn: (id: string) => api.delete(`/api/tasks/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tasks'] })
      toast.success(t('tasks.scheduled.toast_deleted', { defaultValue: 'Task deleted' }))
    },
    onError: () => {
      toast.error(t('tasks.scheduled.toast_delete_failed', { defaultValue: 'Failed to delete task' }))
    }
  })
}

export function useRunScheduledTask() {
  const queryClient = useQueryClient()
  const { t } = useTranslation('settings')
  return useMutation({
    mutationFn: (id: string) => api.post(`/api/tasks/${id}/run`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tasks'] })
      toast.success(t('tasks.scheduled.toast_triggered', { defaultValue: 'Task triggered' }))
    },
    onError: (error: Error & { status?: number }) => {
      if (error.status === 409) {
        toast.error(t('tasks.scheduled.toast_running', { defaultValue: 'Task is currently running' }))
      } else {
        toast.error(t('tasks.scheduled.toast_trigger_failed', { defaultValue: 'Failed to trigger task' }))
      }
    }
  })
}
