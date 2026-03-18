import { Cpu, HardDrive, Network, Server } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { formatBytes } from '@/lib/utils'
import type { DockerContainerStats } from '../types'

interface ContainerStatsProps {
  stats: DockerContainerStats | undefined
}

export function ContainerStats({ stats }: ContainerStatsProps) {
  const cpuDisplay = stats ? `${stats.cpu_percent.toFixed(1)}%` : '-'
  const memoryDisplay = stats ? `${formatBytes(stats.memory_usage)} / ${formatBytes(stats.memory_limit)}` : '-'
  const memoryPercent = stats ? stats.memory_percent.toFixed(1) : null
  const netDisplay = stats ? `${formatBytes(stats.network_rx)} / ${formatBytes(stats.network_tx)}` : '-'
  const blockDisplay = stats ? `${formatBytes(stats.block_read)} / ${formatBytes(stats.block_write)}` : '-'

  return (
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
      <Card size="sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
            <Cpu aria-hidden="true" className="size-4" />
            CPU
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-mono font-semibold text-2xl tabular-nums">{cpuDisplay}</p>
        </CardContent>
      </Card>

      <Card size="sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
            <Server aria-hidden="true" className="size-4" />
            Memory
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-mono font-semibold text-sm tabular-nums">{memoryDisplay}</p>
          {memoryPercent !== null && (
            <div className="mt-2">
              <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                <div
                  className="h-full rounded-full bg-primary transition-all"
                  style={{ width: `${Math.min(Number(memoryPercent), 100)}%` }}
                />
              </div>
              <p className="mt-1 text-muted-foreground text-xs tabular-nums">{memoryPercent}%</p>
            </div>
          )}
        </CardContent>
      </Card>

      <Card size="sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
            <Network aria-hidden="true" className="size-4" />
            Net I/O
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-mono text-muted-foreground text-sm tabular-nums">{netDisplay}</p>
          {stats && (
            <p className="mt-1 text-muted-foreground text-xs">
              <span>rx / tx</span>
            </p>
          )}
        </CardContent>
      </Card>

      <Card size="sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-muted-foreground text-sm">
            <HardDrive aria-hidden="true" className="size-4" />
            Block I/O
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-mono text-muted-foreground text-sm tabular-nums">{blockDisplay}</p>
          {stats && (
            <p className="mt-1 text-muted-foreground text-xs">
              <span>read / write</span>
            </p>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
