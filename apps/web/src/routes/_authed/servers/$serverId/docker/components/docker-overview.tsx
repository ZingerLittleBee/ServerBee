import { Activity, Box, Cpu, HardDrive, Square } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { formatBytes } from '@/lib/utils'
import type { DockerContainer, DockerContainerStats } from '../types'

interface DockerOverviewProps {
  containers: DockerContainer[]
  dockerVersion?: string
  stats: DockerContainerStats[]
}

export function DockerOverview({ containers, stats, dockerVersion }: DockerOverviewProps) {
  const { t } = useTranslation('docker')
  const running = containers.filter((c) => c.state === 'running').length
  const stopped = containers.filter((c) => c.state !== 'running').length

  const totalCpu = stats.reduce((sum, s) => sum + s.cpu_percent, 0)
  const totalMemory = stats.reduce((sum, s) => sum + s.memory_usage, 0)

  return (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-5">
      <Card size="sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
            <Box aria-hidden="true" className="size-4" />
            {t('overview.running')}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-mono font-semibold text-2xl tabular-nums">{running}</p>
        </CardContent>
      </Card>

      <Card size="sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
            <Square aria-hidden="true" className="size-4" />
            {t('overview.stopped')}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-mono font-semibold text-2xl tabular-nums">{stopped}</p>
        </CardContent>
      </Card>

      <Card size="sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
            <Cpu aria-hidden="true" className="size-4" />
            {t('overview.totalCpu')}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-mono font-semibold text-2xl tabular-nums">{totalCpu.toFixed(1)}%</p>
        </CardContent>
      </Card>

      <Card size="sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
            <HardDrive aria-hidden="true" className="size-4" />
            {t('overview.totalMemory')}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-mono font-semibold text-2xl tabular-nums">{formatBytes(totalMemory)}</p>
        </CardContent>
      </Card>

      <Card size="sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
            <Activity aria-hidden="true" className="size-4" />
            {t('overview.dockerVersion')}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-mono font-semibold text-lg tabular-nums">{dockerVersion ?? t('overview.unknown')}</p>
        </CardContent>
      </Card>
    </div>
  )
}
