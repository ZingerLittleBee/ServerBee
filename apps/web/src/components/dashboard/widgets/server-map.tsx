import { useMemo } from 'react'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
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

// SVG viewBox matches the coordinate system: longitude -180..180, latitude -90..90
const VIEW_BOX = '-180 -90 360 180'

export function ServerMapWidget({ config, servers }: ServerMapWidgetProps) {
  const serverIds = config.server_ids

  const filteredServers = useMemo(() => {
    if (!serverIds || serverIds.length === 0) {
      return servers
    }
    const idSet = new Set(serverIds)
    return servers.filter((s) => idSet.has(s.id))
  }, [servers, serverIds])

  // Group servers by country_code
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
        // Find centroid from map data
        const countryPath = WORLD_MAP_PATHS.find((p) => p.id === upper)
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
          {/* Country paths */}
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

          {/* Server markers */}
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
                {/* Ping animation ring */}
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
      {countryGroups.length === 0 && (
        <p className="py-2 text-center text-muted-foreground text-xs">No server location data available</p>
      )}
    </div>
  )
}
