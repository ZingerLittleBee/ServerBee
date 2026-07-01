import { AlertTriangle, Trash2 } from 'lucide-react'
import { Fragment } from 'react'
import { useTranslation } from 'react-i18next'
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
import { Skeleton } from '@/components/ui/skeleton'
import type { AlertRule, AlertRuleItem, AlertStateResponse } from '@/lib/api-schema'

function formatRuleItem(item: AlertRuleItem, t: (key: string, options?: Record<string, unknown>) => string): string {
  if (item.rule_type === 'offline') {
    return `${t('alerts.display_offline')} ${item.duration ?? 60}s`
  }
  if (item.rule_type === 'expiration') {
    return t('alerts.display_expires', { count: item.duration ?? 7 })
  }
  if (item.rule_type === 'ip_changed') {
    return 'IP Changed'
  }
  if (item.rule_type === 'capability_grant_detected') {
    return t('alerts.metric_capability_granted')
  }
  if (item.cycle_limit) {
    return t('alerts.display_transfer', { value: item.cycle_limit, period: item.cycle_interval ?? 'month' })
  }
  if (item.min && item.max) {
    return `${item.rule_type} [${item.min}, ${item.max}]`
  }
  if (item.min) {
    return `${item.rule_type} >= ${item.min}`
  }
  if (item.max) {
    return `${item.rule_type} >= ${item.max}`
  }
  return item.rule_type
}

export function AlertRulesList({
  deletePending,
  deleteRuleId,
  expandedRuleId,
  isLoading,
  onDeleteClose,
  onDeleteConfirm,
  onDeleteOpen,
  onToggleEnabled,
  onToggleExpanded,
  rules,
  states
}: {
  deletePending: boolean
  deleteRuleId: string | null
  expandedRuleId: string | null
  isLoading: boolean
  onDeleteClose: () => void
  onDeleteConfirm: (ruleId: string) => void
  onDeleteOpen: (ruleId: string) => void
  onToggleEnabled: (rule: AlertRule) => void
  onToggleExpanded: (ruleId: string) => void
  rules: AlertRule[] | undefined
  states: AlertStateResponse[] | undefined
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

  if (!rules || rules.length === 0) {
    return <p className="text-center text-muted-foreground text-sm">{t('alerts.no_rules')}</p>
  }

  return (
    <div className="divide-y rounded-md border">
      {rules.map((rule) => {
        const items: AlertRuleItem[] = JSON.parse(rule.rules_json || '[]')
        return (
          <Fragment key={rule.id}>
            <div className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
              <div className="flex min-w-0 items-center gap-3">
                <AlertTriangle className={`size-4 ${rule.enabled ? 'text-amber-500' : 'text-muted-foreground'}`} />
                <div>
                  <p className="font-medium text-sm">
                    {rule.name}
                    {!rule.enabled && (
                      <span className="ml-2 text-muted-foreground text-xs">{t('notifications.disabled')}</span>
                    )}
                    <button
                      className="ml-2 rounded-full bg-muted px-2 py-0.5 text-muted-foreground text-xs hover:bg-muted/80"
                      onClick={(event) => {
                        event.stopPropagation()
                        onToggleExpanded(rule.id)
                      }}
                      type="button"
                    >
                      {t('alerts.states')}
                    </button>
                  </p>
                  <p className="text-muted-foreground text-xs">
                    {items.map((item) => formatRuleItem(item, t)).join(' AND ')} | {rule.trigger_mode}
                  </p>
                </div>
              </div>
              <div className="flex gap-1">
                <Button onClick={() => onToggleEnabled(rule)} size="sm" variant="outline">
                  {rule.enabled ? t('common:disable') : t('common:enable')}
                </Button>
                <AlertDialog onOpenChange={(open) => !open && onDeleteClose()} open={deleteRuleId === rule.id}>
                  <AlertDialogTrigger
                    onClick={() => onDeleteOpen(rule.id)}
                    render={
                      <Button
                        aria-label={`${t('common:delete')} ${rule.name}`}
                        disabled={deletePending}
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
                      <AlertDialogAction onClick={() => onDeleteConfirm(rule.id)} variant="destructive">
                        {t('common:delete')}
                      </AlertDialogAction>
                    </AlertDialogFooter>
                  </AlertDialogContent>
                </AlertDialog>
              </div>
            </div>
            {expandedRuleId === rule.id && <AlertRuleStates states={states} />}
          </Fragment>
        )
      })}
    </div>
  )
}

function AlertRuleStates({ states }: { states: AlertStateResponse[] | undefined }) {
  const { t } = useTranslation(['settings', 'common'])

  return (
    <div className="border-t bg-muted/20 px-4 py-2">
      {states && states.length > 0 ? (
        <div className="space-y-1">
          {states.map((state) => (
            <div className="flex items-center justify-between text-xs" key={state.server_id}>
              <span className="flex items-center gap-2">
                <span className={`size-2 rounded-full ${state.resolved ? 'bg-green-500' : 'bg-red-500'}`} />
                {state.server_name}
              </span>
              <span className="text-muted-foreground">
                {state.resolved ? t('alerts.resolved') : `${t('alerts.triggered')} (${state.count}x)`}
                {' · '}
                {new Date(state.first_triggered_at).toLocaleString()}
              </span>
            </div>
          ))}
        </div>
      ) : (
        <p className="text-muted-foreground text-xs">{t('alerts.no_triggered')}</p>
      )}
    </div>
  )
}
