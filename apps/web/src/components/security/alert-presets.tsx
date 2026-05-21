import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Ban, ShieldAlert, UserCheck } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
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
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { api } from '@/lib/api-client'
import type { AlertRule, AlertRuleItem, NotificationGroup } from '@/lib/api-schema'

type PresetKind = 'ssh_brute_force_detected' | 'ssh_new_ip_login' | 'port_scan_detected'

interface PresetDef {
  defaultName: string
  descriptionDefault: string
  descriptionKey: string
  icon: typeof ShieldAlert
  kind: PresetKind
  titleDefault: string
  titleKey: string
}

const PRESETS: PresetDef[] = [
  {
    kind: 'ssh_brute_force_detected',
    icon: ShieldAlert,
    titleKey: 'preset.brute_force_title',
    titleDefault: 'SSH Brute Force',
    descriptionKey: 'preset.brute_force_description',
    descriptionDefault: 'Notify when the agent detects an SSH brute-force burst on a server.',
    defaultName: 'SSH Brute Force'
  },
  {
    kind: 'ssh_new_ip_login',
    icon: UserCheck,
    titleKey: 'preset.new_ip_login_title',
    titleDefault: 'New SSH Source',
    descriptionKey: 'preset.new_ip_login_description',
    descriptionDefault: 'Notify when a successful SSH login arrives from a previously unseen (user, IP) pair.',
    defaultName: 'New SSH Source'
  },
  {
    kind: 'port_scan_detected',
    icon: Ban,
    titleKey: 'preset.port_scan_title',
    titleDefault: 'Port Scan',
    descriptionKey: 'preset.port_scan_description',
    descriptionDefault: 'Notify when the agent detects a port scan against this server.',
    defaultName: 'Port Scan'
  }
]

interface CreateAlertInput {
  cover_type: string
  name: string
  notification_group_id: string | null
  rules: AlertRuleItem[]
  server_ids: string[]
  trigger_mode: string
}

interface SecurityFormState {
  dedupe: string
  excludeCidrs: string
  excludeUsers: string
  minDistinctPorts: string
  minFailedCount: string
}

function parsePositive(value: string): number | null {
  const parsed = Number(value)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null
}

function splitCsv(value: string): string[] {
  return value
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
}

function buildSecurityParams(kind: PresetKind, state: SecurityFormState): Record<string, unknown> {
  const security: Record<string, unknown> = {
    dedupe_window_seconds: parsePositive(state.dedupe) ?? 600
  }
  if (kind === 'ssh_brute_force_detected') {
    const v = parsePositive(state.minFailedCount)
    if (v !== null) {
      security.min_failed_count = v
    }
  }
  if (kind === 'port_scan_detected') {
    const v = parsePositive(state.minDistinctPorts)
    if (v !== null) {
      security.min_distinct_ports = v
    }
  }
  if (kind === 'ssh_new_ip_login') {
    const users = splitCsv(state.excludeUsers)
    const cidrs = splitCsv(state.excludeCidrs)
    if (users.length > 0) {
      security.exclude_users = users
    }
    if (cidrs.length > 0) {
      security.exclude_cidrs = cidrs
    }
  }
  return security
}

export function SecurityAlertPresets() {
  const { t } = useTranslation('security')

  return (
    <div className="space-y-3">
      <div>
        <h3 className="font-medium text-sm">{t('preset.section_title', { defaultValue: 'Security presets' })}</h3>
        <p className="text-muted-foreground text-xs">
          {t('preset.section_hint', {
            defaultValue: 'Quick-create rules driven by SecurityEvent reports from agents.'
          })}
        </p>
      </div>
      <div className="grid gap-3 md:grid-cols-3">
        {PRESETS.map((preset) => (
          <PresetCard key={preset.kind} preset={preset} />
        ))}
      </div>
    </div>
  )
}

function PresetCard({ preset }: { preset: PresetDef }) {
  const { t } = useTranslation('security')
  const Icon = preset.icon

  return (
    <div className="flex flex-col gap-3 rounded-md border bg-card p-4">
      <div className="flex items-start gap-2">
        <Icon aria-hidden="true" className="mt-0.5 size-4 text-primary" />
        <div className="min-w-0">
          <p className="font-medium text-sm">{t(preset.titleKey, { defaultValue: preset.titleDefault })}</p>
          <p className="mt-1 text-muted-foreground text-xs">
            {t(preset.descriptionKey, { defaultValue: preset.descriptionDefault })}
          </p>
        </div>
      </div>
      <PresetDialog preset={preset} />
    </div>
  )
}

function PresetDialog({ preset }: { preset: PresetDef }) {
  const { t } = useTranslation('security')
  const [open, setOpen] = useState(false)
  const [name, setName] = useState(preset.defaultName)
  const [groupId, setGroupId] = useState<string>('')
  const [minFailedCount, setMinFailedCount] = useState<string>('20')
  const [minDistinctPorts, setMinDistinctPorts] = useState<string>('50')
  const [excludeUsers, setExcludeUsers] = useState<string>('')
  const [excludeCidrs, setExcludeCidrs] = useState<string>('')
  const [dedupe, setDedupe] = useState<string>('600')

  const queryClient = useQueryClient()

  const { data: groups } = useQuery<NotificationGroup[]>({
    queryKey: ['notification-groups'],
    queryFn: () => api.get<NotificationGroup[]>('/api/notification-groups'),
    enabled: open
  })

  const createMutation = useMutation({
    mutationFn: (input: CreateAlertInput) => api.post<AlertRule>('/api/alert-rules', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['alert-rules'] }).catch(() => undefined)
      toast.success(t('preset.created', { defaultValue: 'Alert rule created' }))
      setOpen(false)
    },
    onError: (err) =>
      toast.error(err instanceof Error ? err.message : t('preset.create_failed', { defaultValue: 'Create failed' }))
  })

  const submit = () => {
    if (name.trim().length === 0) {
      toast.error(t('preset.name_required', { defaultValue: 'Name required' }))
      return
    }

    const security = buildSecurityParams(preset.kind, {
      dedupe,
      minFailedCount,
      minDistinctPorts,
      excludeUsers,
      excludeCidrs
    })

    createMutation.mutate({
      cover_type: 'all',
      name: name.trim(),
      notification_group_id: groupId || null,
      rules: [{ rule_type: preset.kind, security: security as AlertRuleItem['security'] }],
      server_ids: [],
      trigger_mode: 'always'
    })
  }

  return (
    <Dialog onOpenChange={setOpen} open={open}>
      <DialogTrigger
        render={
          <Button size="sm" variant="outline">
            {t('preset.configure', { defaultValue: 'Configure' })}
          </Button>
        }
      />
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t(preset.titleKey, { defaultValue: preset.titleDefault })}</DialogTitle>
          <DialogDescription>{t(preset.descriptionKey, { defaultValue: preset.descriptionDefault })}</DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          <div className="space-y-1">
            <Label htmlFor={`preset-name-${preset.kind}`}>
              {t('preset.field_name', { defaultValue: 'Rule name' })}
            </Label>
            <Input
              id={`preset-name-${preset.kind}`}
              onChange={(e) => setName(e.target.value)}
              placeholder={preset.defaultName}
              value={name}
            />
          </div>

          <div className="space-y-1">
            <Label htmlFor={`preset-group-${preset.kind}`}>
              {t('preset.field_group', { defaultValue: 'Notification group' })}
            </Label>
            <Select onValueChange={setGroupId} value={groupId}>
              <SelectTrigger id={`preset-group-${preset.kind}`}>
                <SelectValue placeholder={t('preset.field_group_none', { defaultValue: 'None' })} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="">{t('preset.field_group_none', { defaultValue: 'None' })}</SelectItem>
                {groups?.map((g) => (
                  <SelectItem key={g.id} value={g.id}>
                    {g.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {preset.kind === 'ssh_brute_force_detected' && (
            <div className="space-y-1">
              <Label htmlFor={`preset-min-failed-${preset.kind}`}>
                {t('preset.field_min_failed', { defaultValue: 'Minimum failed attempts' })}
              </Label>
              <Input
                id={`preset-min-failed-${preset.kind}`}
                inputMode="numeric"
                onChange={(e) => setMinFailedCount(e.target.value)}
                value={minFailedCount}
              />
            </div>
          )}

          {preset.kind === 'port_scan_detected' && (
            <div className="space-y-1">
              <Label htmlFor={`preset-min-ports-${preset.kind}`}>
                {t('preset.field_min_ports', { defaultValue: 'Minimum distinct ports' })}
              </Label>
              <Input
                id={`preset-min-ports-${preset.kind}`}
                inputMode="numeric"
                onChange={(e) => setMinDistinctPorts(e.target.value)}
                value={minDistinctPorts}
              />
            </div>
          )}

          {preset.kind === 'ssh_new_ip_login' && (
            <>
              <div className="space-y-1">
                <Label htmlFor={`preset-exclude-users-${preset.kind}`}>
                  {t('preset.field_exclude_users', { defaultValue: 'Exclude users (comma-separated)' })}
                </Label>
                <Input
                  id={`preset-exclude-users-${preset.kind}`}
                  onChange={(e) => setExcludeUsers(e.target.value)}
                  placeholder="nagios, backup"
                  value={excludeUsers}
                />
              </div>
              <div className="space-y-1">
                <Label htmlFor={`preset-exclude-cidrs-${preset.kind}`}>
                  {t('preset.field_exclude_cidrs', { defaultValue: 'Exclude CIDRs (comma-separated)' })}
                </Label>
                <Input
                  id={`preset-exclude-cidrs-${preset.kind}`}
                  onChange={(e) => setExcludeCidrs(e.target.value)}
                  placeholder="10.0.0.0/8"
                  value={excludeCidrs}
                />
              </div>
            </>
          )}

          <div className="space-y-1">
            <Label htmlFor={`preset-dedupe-${preset.kind}`}>
              {t('preset.field_dedupe', { defaultValue: 'Dedupe window (seconds)' })}
            </Label>
            <Input
              id={`preset-dedupe-${preset.kind}`}
              inputMode="numeric"
              onChange={(e) => setDedupe(e.target.value)}
              value={dedupe}
            />
          </div>
        </div>

        <DialogFooter>
          <Button onClick={() => setOpen(false)} variant="outline">
            {t('preset.cancel', { defaultValue: 'Cancel' })}
          </Button>
          <Button disabled={createMutation.isPending} onClick={submit}>
            {createMutation.isPending
              ? t('preset.creating', { defaultValue: 'Creating…' })
              : t('preset.create', { defaultValue: 'Create rule' })}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
