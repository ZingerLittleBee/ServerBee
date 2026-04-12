import { Search } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { cn, formatBytes } from '@/lib/utils'
import type { DockerContainer, DockerContainerStats } from '../types'

type FilterState = 'all' | 'running' | 'stopped'

interface ContainerListProps {
  containers: DockerContainer[]
  onSelect?: (container: DockerContainer) => void
  stats: DockerContainerStats[]
}

function getStatsForContainer(containerId: string, stats: DockerContainerStats[]): DockerContainerStats | undefined {
  return stats.find((s) => s.id === containerId)
}

function formatNetworkIO(rx: number, tx: number): string {
  return `${formatBytes(rx)} / ${formatBytes(tx)}`
}

export function ContainerList({ containers, stats, onSelect }: ContainerListProps) {
  const { t } = useTranslation('docker')
  const [search, setSearch] = useState('')
  const [filter, setFilter] = useState<FilterState>('all')

  const filteredContainers = useMemo(() => {
    const query = search.toLowerCase().trim()

    return containers.filter((container) => {
      if (filter === 'running' && container.state !== 'running') {
        return false
      }
      if (filter === 'stopped' && container.state === 'running') {
        return false
      }

      if (query) {
        const nameMatch = container.name.toLowerCase().includes(query)
        const imageMatch = container.image.toLowerCase().includes(query)
        return nameMatch || imageMatch
      }

      return true
    })
  }, [containers, search, filter])

  const runningCount = useMemo(() => containers.filter((c) => c.state === 'running').length, [containers])
  const stoppedCount = useMemo(() => containers.filter((c) => c.state !== 'running').length, [containers])

  return (
    <div className="space-y-3">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h3 className="font-semibold text-lg">{t('containers.title')}</h3>
        <div className="flex items-center gap-2">
          <div className="relative">
            <Search
              aria-hidden="true"
              className="absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
            />
            <Input
              className="w-[200px] pl-8"
              onChange={(e) => setSearch(e.target.value)}
              placeholder={t('containers.searchPlaceholder')}
              value={search}
            />
          </div>
          <div className="flex gap-1">
            <Button onClick={() => setFilter('all')} size="sm" variant={filter === 'all' ? 'default' : 'outline'}>
              {t('filter.all')} ({containers.length})
            </Button>
            <Button
              onClick={() => setFilter('running')}
              size="sm"
              variant={filter === 'running' ? 'default' : 'outline'}
            >
              {t('filter.running')} ({runningCount})
            </Button>
            <Button
              onClick={() => setFilter('stopped')}
              size="sm"
              variant={filter === 'stopped' ? 'default' : 'outline'}
            >
              {t('filter.stopped')} ({stoppedCount})
            </Button>
          </div>
        </div>
      </div>

      <div className="rounded-lg border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t('table.name')}</TableHead>
              <TableHead>{t('table.image')}</TableHead>
              <TableHead>{t('table.status')}</TableHead>
              <TableHead className="text-right">{t('table.cpu')}</TableHead>
              <TableHead className="text-right">{t('table.memory')}</TableHead>
              <TableHead className="text-right">{t('table.networkIO')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {filteredContainers.length === 0 ? (
              <TableRow>
                <TableCell className="text-center text-muted-foreground" colSpan={6}>
                  {search || filter !== 'all' ? t('containers.noFilterMatch') : t('containers.noContainers')}
                </TableCell>
              </TableRow>
            ) : (
              filteredContainers.map((container) => {
                const containerStats = getStatsForContainer(container.id, stats)

                return (
                  <TableRow
                    className={cn(onSelect && 'cursor-pointer')}
                    key={container.id}
                    onClick={() => onSelect?.(container)}
                    onKeyDown={(e) => {
                      if ((e.key === 'Enter' || e.key === ' ') && onSelect) {
                        e.preventDefault()
                        onSelect(container)
                      }
                    }}
                    tabIndex={onSelect ? 0 : undefined}
                  >
                    <TableCell className="font-medium">{container.name}</TableCell>
                    <TableCell className="max-w-[200px] truncate text-muted-foreground">{container.image}</TableCell>
                    <TableCell>
                      <Badge variant={container.state === 'running' ? 'default' : 'secondary'}>{container.state}</Badge>
                    </TableCell>
                    <TableCell className="text-right font-mono tabular-nums">
                      {containerStats ? `${containerStats.cpu_percent.toFixed(1)}%` : '-'}
                    </TableCell>
                    <TableCell className="text-right font-mono tabular-nums">
                      {containerStats ? formatBytes(containerStats.memory_usage) : '-'}
                    </TableCell>
                    <TableCell className="text-right font-mono text-muted-foreground tabular-nums">
                      {containerStats ? formatNetworkIO(containerStats.network_rx, containerStats.network_tx) : '-'}
                    </TableCell>
                  </TableRow>
                )
              })
            )}
          </TableBody>
        </Table>
      </div>
    </div>
  )
}
