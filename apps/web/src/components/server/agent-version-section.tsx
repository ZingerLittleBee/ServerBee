import { CheckCircle2, CircleAlert, Clock, Download, Loader2, RefreshCw, ShieldCheck, Wrench } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { useAuth } from '@/hooks/use-auth'
import { useUpgradeJob } from '@/hooks/use-upgrade-job'
import { CAP_UPGRADE, getEffectiveCapabilityEnabled } from '@/lib/capabilities'
import type { UpgradeJob, UpgradeStage } from '@/stores/upgrade-jobs-store'

interface AgentVersionSectionProps {
  agentVersion: string | null | undefined
  configuredCapabilities?: number | null
  effectiveCapabilities?: number | null
  latestVersion: string | null | undefined
  serverId: string
}

const STAGE_ORDER: UpgradeStage[] = ['downloading', 'verifying', 'pre_flight', 'installing', 'restarting']

const STAGE_ICONS: Record<UpgradeStage, typeof Download> = {
  downloading: Download,
  verifying: ShieldCheck,
  pre_flight: Wrench,
  installing: RefreshCw,
  restarting: RefreshCw
}

function getStageProgress(stage: UpgradeStage): number {
  const index = STAGE_ORDER.indexOf(stage)
  return ((index + 1) / STAGE_ORDER.length) * 100
}

function UpgradeStepper({ job, t }: { job: UpgradeJob; t: (key: string) => string }) {
  const currentIndex = STAGE_ORDER.indexOf(job.stage)

  return (
    <div className="mt-4 space-y-3">
      <div className="flex items-center gap-2 text-sm">
        <Loader2 className="size-4 animate-spin text-primary" />
        <span className="font-medium">{t('upgrade_in_progress')}</span>
        <Badge variant="secondary">{t(`upgrade_stage_${job.stage}`)}</Badge>
      </div>

      <div className="relative">
        <div className="absolute top-1/2 h-1 w-full -translate-y-1/2 rounded-full bg-muted" />
        <div
          className="absolute top-1/2 h-1 -translate-y-1/2 rounded-full bg-primary transition-all duration-500"
          style={{ width: `${getStageProgress(job.stage)}%` }}
        />
        <div className="relative flex justify-between">
          {STAGE_ORDER.map((stage, index) => {
            const Icon = STAGE_ICONS[stage]
            const isActive = index <= currentIndex
            const isCurrent = index === currentIndex

            return (
              <div
                className={`flex flex-col items-center gap-1 ${isActive ? 'text-primary' : 'text-muted-foreground'}`}
                key={stage}
              >
                <div
                  className={`flex size-8 items-center justify-center rounded-full border-2 bg-background transition-colors ${
                    isActive ? 'border-primary' : 'border-muted'
                  } ${isCurrent ? 'ring-2 ring-primary/20' : ''}`}
                >
                  <Icon className={`size-4 ${isCurrent ? 'animate-pulse' : ''}`} />
                </div>
                <span className={`text-[10px] ${isCurrent ? 'font-medium' : ''}`}>{t(`upgrade_stage_${stage}`)}</span>
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}

function UpgradeSuccess({ job, t }: { job: UpgradeJob; t: (key: string) => string }) {
  return (
    <div className="mt-4 flex items-start gap-3 rounded-lg border border-emerald-200 bg-emerald-50/80 p-3 dark:border-emerald-900/50 dark:bg-emerald-950/30">
      <CheckCircle2 className="mt-0.5 size-5 shrink-0 text-emerald-600 dark:text-emerald-400" />
      <div className="flex-1">
        <p className="font-medium text-emerald-900 text-sm dark:text-emerald-300">{t('upgrade_status_succeeded')}</p>
        <p className="mt-1 text-emerald-800/80 text-xs dark:text-emerald-400/80">
          {t('upgrade_current_version')}: v{job.target_version}
        </p>
      </div>
    </div>
  )
}

function UpgradeFailed({ job, t }: { job: UpgradeJob; t: (key: string) => string }) {
  return (
    <div className="mt-4 flex items-start gap-3 rounded-lg border border-red-200 bg-red-50/80 p-3 dark:border-red-900/50 dark:bg-red-950/30">
      <CircleAlert className="mt-0.5 size-5 shrink-0 text-red-600 dark:text-red-400" />
      <div className="flex-1">
        <p className="font-medium text-red-900 text-sm dark:text-red-300">{t('upgrade_status_failed')}</p>
        {job.error && <p className="mt-1 text-red-800/80 text-xs dark:text-red-400/80">{job.error}</p>}
        {job.backup_path && (
          <p className="mt-2 text-red-700 text-xs dark:text-red-400/70">
            {t('upgrade_error_with_backup')}: {job.backup_path}
          </p>
        )}
      </div>
    </div>
  )
}

function UpgradeTimeout({ job, t }: { job: UpgradeJob; t: (key: string) => string }) {
  return (
    <div className="mt-4 flex items-start gap-3 rounded-lg border border-amber-200 bg-amber-50/80 p-3 dark:border-amber-900/50 dark:bg-amber-950/30">
      <Clock className="mt-0.5 size-5 shrink-0 text-amber-600 dark:text-amber-400" />
      <div className="flex-1">
        <p className="font-medium text-amber-900 text-sm dark:text-amber-300">{t('upgrade_status_timeout')}</p>
        {job.backup_path && (
          <p className="mt-2 text-amber-700 text-xs dark:text-amber-400/70">
            {t('upgrade_backup_path')}: {job.backup_path}
          </p>
        )}
      </div>
    </div>
  )
}

export function AgentVersionSection({
  agentVersion,
  latestVersion,
  serverId,
  effectiveCapabilities,
  configuredCapabilities
}: AgentVersionSectionProps) {
  const { t } = useTranslation('servers')
  const { user } = useAuth()
  const { job, triggerUpgrade, isLoading } = useUpgradeJob(serverId)

  const isAdmin = user?.role === 'admin'
  const upgradeEnabled = getEffectiveCapabilityEnabled(effectiveCapabilities, configuredCapabilities, CAP_UPGRADE)

  const hasUpdate = latestVersion && agentVersion && latestVersion !== agentVersion
  const canUpgrade = isAdmin && upgradeEnabled && hasUpdate && (!job || job.status !== 'running')

  const handleUpgrade = () => {
    if (latestVersion) {
      triggerUpgrade(latestVersion)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('cap_upgrade')}</CardTitle>
        <CardDescription>{t('upgrade_current_version')}</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex items-center justify-between">
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <span className="font-mono text-lg">v{agentVersion || 'unknown'}</span>
              {hasUpdate && (
                <Badge variant="secondary">
                  {t('upgrade_latest_version')}: v{latestVersion}
                </Badge>
              )}
            </div>
            {!upgradeEnabled && isAdmin && <p className="text-muted-foreground text-xs">{t('cap_disabled')}</p>}
          </div>

          {canUpgrade && (
            <Button disabled={isLoading} onClick={handleUpgrade} size="sm">
              {isLoading ? <Loader2 className="mr-1 size-4 animate-spin" /> : <RefreshCw className="mr-1 size-4" />}
              {t('upgrade_start')}
            </Button>
          )}
        </div>

        {job?.status === 'running' && <UpgradeStepper job={job} t={t} />}
        {job?.status === 'succeeded' && <UpgradeSuccess job={job} t={t} />}
        {job?.status === 'failed' && <UpgradeFailed job={job} t={t} />}
        {job?.status === 'timeout' && <UpgradeTimeout job={job} t={t} />}
      </CardContent>
    </Card>
  )
}
