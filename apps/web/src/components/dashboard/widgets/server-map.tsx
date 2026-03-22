import { useMutation, useQuery } from '@tanstack/react-query'
import { Download } from 'lucide-react'
import { useMemo } from 'react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { useAuth } from '@/hooks/use-auth'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import { filterByIds } from '@/lib/widget-helpers'
import type { ServerMapConfig } from '@/lib/widget-types'
import { WORLD_MAP_PATHS } from '@/lib/world-map-paths'

interface ServerMapWidgetProps {
  config: ServerMapConfig
  servers: ServerMetrics[]
}

interface CountryGroup {
  count: number
  countryCode: string
  cx: number
  cy: number
  name: string
  serverNames: string[]
}

const VIEW_BOX = '-180 -90 360 180'

const COUNTRY_MAP = new Map(WORLD_MAP_PATHS.map((p) => [p.id, p]))

export function ServerMapWidget({ config, servers }: ServerMapWidgetProps) {
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  const { data: geoStatus } = useQuery<{ installed: boolean; source?: string }>({
    queryKey: ['geoip-status'],
    queryFn: () => api.get('/api/geoip/status')
  })

  const downloadMutation = useMutation({
    mutationFn: () => api.post<{ success: boolean; message: string }>('/api/geoip/download'),
    onSuccess: (data) => {
      if (data.success) {
        toast.success(data.message)
      } else {
        toast.error(data.message)
      }
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Download failed')
    }
  })

  const filteredServers = useMemo(
    () => filterByIds(servers, config.server_ids, (s) => s.id),
    [servers, config.server_ids]
  )

  const countryGroups = useMemo(() => {
    const groups = new Map<string, CountryGroup>()

    for (const server of filteredServers) {
      const code = server.country_code
      if (!code) {
        continue
      }
      const upper = code.toUpperCase()
      const existing = groups.get(upper)
      if (existing) {
        existing.count++
        existing.serverNames.push(server.name)
      } else {
        const countryPath = COUNTRY_MAP.get(upper)
        if (countryPath) {
          groups.set(upper, {
            countryCode: upper,
            name: countryPath.name,
            cx: countryPath.cx,
            cy: countryPath.cy,
            count: 1,
            serverNames: [server.name]
          })
        }
      }
    }

    return Array.from(groups.values())
  }, [filteredServers])

  const highlightedCountries = useMemo(() => {
    return new Set(countryGroups.map((g) => g.countryCode))
  }, [countryGroups])

  const maxCount = useMemo(() => {
    return Math.max(1, ...countryGroups.map((g) => g.count))
  }, [countryGroups])

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-3">
      <h3 className="mb-2 font-semibold text-sm">Server Map</h3>
      <div className="flex-1 overflow-hidden">
        <svg
          aria-label="Server map"
          className="h-full w-full"
          preserveAspectRatio="xMidYMid meet"
          role="img"
          viewBox={VIEW_BOX}
        >
          <title>Server Map</title>
          {WORLD_MAP_PATHS.map((country) => (
            <path
              className="transition-colors"
              d={country.d}
              fill={highlightedCountries.has(country.id) ? 'var(--color-chart-1)' : 'var(--color-muted)'}
              key={country.id}
              opacity={highlightedCountries.has(country.id) ? 0.7 : 0.4}
              stroke="var(--color-border)"
              strokeWidth={0.5}
            >
              <title>{country.name}</title>
            </path>
          ))}

          {countryGroups.map((group) => {
            const radius = 1.5 + (group.count / maxCount) * 3
            return (
              <g key={group.countryCode}>
                <circle
                  cx={group.cx}
                  cy={group.cy}
                  fill="var(--color-chart-2)"
                  opacity={0.8}
                  r={radius}
                  stroke="var(--color-background)"
                  strokeWidth={0.5}
                >
                  <title>
                    {group.name}: {group.count} server{group.count > 1 ? 's' : ''}
                    {'\n'}
                    {group.serverNames.join(', ')}
                  </title>
                </circle>
                <circle
                  cx={group.cx}
                  cy={group.cy}
                  fill="none"
                  opacity={0.3}
                  r={radius + 1}
                  stroke="var(--color-chart-2)"
                  strokeWidth={0.3}
                />
              </g>
            )
          })}
        </svg>
      </div>
      {countryGroups.length === 0 &&
        (geoStatus?.installed === false ? (
          <div className="space-y-2 py-2 text-center">
            <p className="text-muted-foreground text-xs">GeoIP database not installed</p>
            {isAdmin && (
              <Button
                disabled={downloadMutation.isPending}
                onClick={() => downloadMutation.mutate()}
                size="sm"
                variant="outline"
              >
                <Download className="mr-1 size-3.5" />
                {downloadMutation.isPending ? 'Downloading...' : 'Download GeoIP Database'}
              </Button>
            )}
          </div>
        ) : (
          <p className="py-2 text-center text-muted-foreground text-xs">No server location data available</p>
        ))}
      {countryGroups.length > 0 && <p className="text-right text-[10px] text-muted-foreground">GeoIP by DB-IP</p>}
    </div>
  )
}
