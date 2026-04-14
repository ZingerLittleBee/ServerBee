import { CircleAlert, Clock, Loader2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import type { UpgradeJob, UpgradeStatus } from '@/stores/upgrade-jobs-store'

interface UpgradeJobBadgeProps {
  job: UpgradeJob | null | undefined
}

const STATUS_CONFIG: Record<
  UpgradeStatus,
  { icon: typeof Loader2; label: string; variant: 'default' | 'secondary' | 'destructive' | 'outline' }
> = {
  running: { icon: Loader2, label: 'upgrade_status_running', variant: 'secondary' },
  succeeded: { icon: Loader2, label: 'upgrade_status_succeeded', variant: 'default' },
  failed: { icon: CircleAlert, label: 'upgrade_status_failed', variant: 'destructive' },
  timeout: { icon: Clock, label: 'upgrade_status_timeout', variant: 'outline' }
}

export function UpgradeJobBadge({ job }: UpgradeJobBadgeProps) {
  const { t } = useTranslation('servers')

  if (!job) {
    return null
  }

  const config = STATUS_CONFIG[job.status]
  const Icon = config.icon

  return (
    <TooltipProvider delayDuration={100}>
      <Tooltip>
        <TooltipTrigger asChild>
          <Badge className="gap-1" variant={config.variant}>
            <Icon className={`size-3 ${job.status === 'running' ? 'animate-spin' : ''}`} />
            {job.status === 'running' && t(`upgrade_stage_${job.stage}`)}
          </Badge>
        </TooltipTrigger>
        <TooltipContent side="top">
          <div className="space-y-1">
            <p className="font-medium">{t(config.label)}</p>
            {job.target_version && (
              <p className="text-muted-foreground text-xs">
                v{job.target_version}
                {job.status === 'running' && ` (${t(`upgrade_stage_${job.stage}`)})`}
              </p>
            )}
          </div>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  )
}
