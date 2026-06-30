import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Plus } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { SecurityAlertPresets } from '@/components/security/alert-presets'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api-client'
import type { AlertRule, AlertStateResponse, NotificationGroup } from '@/lib/api-schema'
import { AlertRuleForm, type CreateAlertRuleInput } from './alert-rule-form'
import { AlertRulesList } from './alert-rules-list'

export const Route = createFileRoute('/_authed/settings/alerts')({
  component: AlertsPage
})

interface Server {
  id: string
  name: string
}

function AlertsPage() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const [showForm, setShowForm] = useState(false)
  const [expandedRuleId, setExpandedRuleId] = useState<string | null>(null)
  const [deleteRuleId, setDeleteRuleId] = useState<string | null>(null)

  const { data: rules, isLoading } = useQuery<AlertRule[]>({
    queryKey: ['alert-rules'],
    queryFn: () => api.get<AlertRule[]>('/api/alert-rules')
  })

  const { data: groups } = useQuery<NotificationGroup[]>({
    queryKey: ['notification-groups'],
    queryFn: () => api.get<NotificationGroup[]>('/api/notification-groups')
  })

  const { data: servers } = useQuery<Server[]>({
    queryKey: ['servers'],
    queryFn: () => api.get<Server[]>('/api/servers'),
    enabled: showForm
  })

  const { data: states } = useQuery<AlertStateResponse[]>({
    queryKey: ['alert-rule-states', expandedRuleId],
    queryFn: () => api.get<AlertStateResponse[]>(`/api/alert-rules/${expandedRuleId}/states`),
    enabled: !!expandedRuleId,
    refetchInterval: 10_000
  })

  const createMutation = useMutation({
    mutationFn: (input: CreateAlertRuleInput) => api.post<AlertRule>('/api/alert-rules', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['alert-rules'] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['alert-rule-states'] }).catch(() => undefined)
      setShowForm(false)
      toast.success(t('alerts.created'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('alerts.create_failed'))
    }
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.delete(`/api/alert-rules/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['alert-rules'] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['alert-rule-states'] }).catch(() => undefined)
      toast.success(t('alerts.deleted'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('alerts.delete_failed'))
    }
  })

  const toggleMutation = useMutation({
    mutationFn: ({ enabled, id }: { enabled: boolean; id: string }) =>
      api.put<AlertRule>(`/api/alert-rules/${id}`, { enabled }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['alert-rules'] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['alert-rule-states'] }).catch(() => undefined)
      toast.success(t('alerts.updated'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('alerts.update_failed'))
    }
  })

  return (
    <div>
      <div className="max-w-4xl space-y-6">
        <SecurityAlertPresets />

        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <h2 className="font-semibold text-lg">{t('alerts.rules')}</h2>
          <Button onClick={() => setShowForm(!showForm)} size="sm" variant="outline">
            <Plus className="size-4" />
            {t('common:add')}
          </Button>
        </div>

        {showForm && (
          <AlertRuleForm
            createPending={createMutation.isPending}
            groups={groups}
            onCancel={() => setShowForm(false)}
            onSubmit={(input) => createMutation.mutate(input)}
            servers={servers}
          />
        )}

        <AlertRulesList
          deletePending={deleteMutation.isPending}
          deleteRuleId={deleteRuleId}
          expandedRuleId={expandedRuleId}
          isLoading={isLoading}
          onDeleteClose={() => setDeleteRuleId(null)}
          onDeleteConfirm={(ruleId) => {
            deleteMutation.mutate(ruleId)
            setDeleteRuleId(null)
          }}
          onDeleteOpen={setDeleteRuleId}
          onToggleEnabled={(rule) => toggleMutation.mutate({ id: rule.id, enabled: !rule.enabled })}
          onToggleExpanded={(ruleId) => setExpandedRuleId(expandedRuleId === ruleId ? null : ruleId)}
          rules={rules}
          states={states}
        />
      </div>
    </div>
  )
}
