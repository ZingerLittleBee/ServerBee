import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { ChartStatFlow } from '@/components/charts/chart-stat-flow'
import { ChoroplethChart } from '@/components/charts/choropleth/choropleth-chart'
import { type ChoroplethFeature, useChoropleth } from '@/components/charts/choropleth/choropleth-context'
import { ChoroplethFeature as ChoroplethFeatureComponent } from '@/components/charts/choropleth/choropleth-feature'
import { ChoroplethTooltip } from '@/components/charts/choropleth/choropleth-tooltip'
import { TooltipContent } from '@/components/charts/tooltip/tooltip-content'
import { Button } from '@/components/ui/button'
import { useAuth } from '@/hooks/use-auth'
import { api } from '@/lib/api-client'
import type { ServerMetrics } from '@/lib/server-catalog'
import { alpha3ToAlpha2, buildCountryServerGroups, countryServerFill } from '@/lib/server-geo'
import { useWorldDataStandalone } from '@/lib/use-world-data'
import { countryCodeToName } from '@/lib/utils'
import { filterByIds } from '@/lib/widget-helpers'
import type { ServerMapConfig } from '@/lib/widget-types'

interface ServerMapWidgetProps {
  config: ServerMapConfig
  servers: ServerMetrics[]
}

interface HoveredCountry {
  alpha3: string
  name: string
}

/** Syncs the hovered choropleth feature into the widget header stat. */
function ServerMapHoverBridge({ onHoverChange }: { onHoverChange: (hover: HoveredCountry | null) => void }) {
  const { tooltipData } = useChoropleth()

  useEffect(() => {
    const feature = tooltipData?.feature
    if (!feature) {
      onHoverChange(null)
      return
    }
    const alpha3 = feature.id == null ? '' : String(feature.id)
    const name = (feature.properties?.name as string | undefined) ?? alpha3
    onHoverChange({ alpha3, name })
  }, [onHoverChange, tooltipData])

  return null
}

export function ServerMapWidget({ config, servers }: ServerMapWidgetProps) {
  const { t, i18n } = useTranslation('dashboard')
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'
  const queryClient = useQueryClient()

  const { worldData, isLoading } = useWorldDataStandalone()
  const [hovered, setHovered] = useState<HoveredCountry | null>(null)

  const { data: geoStatus } = useQuery<{ installed: boolean; source?: string }>({
    queryKey: ['geoip-status'],
    queryFn: () => api.get('/api/geoip/status')
  })

  const downloadMutation = useMutation({
    mutationFn: () => api.post<{ success: boolean; message: string }>('/api/geoip/download'),
    onSuccess: (data) => {
      if (data.success) {
        toast.success(data.message)
        queryClient.invalidateQueries({ queryKey: ['geoip-status'] })
      } else {
        toast.error(data.message)
      }
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('states.download_failed'))
    }
  })

  const filteredServers = useMemo(
    () => filterByIds(servers, config.server_ids, (s) => s.id),
    [servers, config.server_ids]
  )

  const countryGroups = useMemo(() => buildCountryServerGroups(filteredServers), [filteredServers])

  const maxCount = useMemo(() => {
    return Math.max(1, ...Array.from(countryGroups.values()).map((g) => g.count))
  }, [countryGroups])

  const totalLocated = useMemo(() => {
    let total = 0
    for (const group of countryGroups.values()) {
      total += group.count
    }
    return total
  }, [countryGroups])

  const getFeatureGroup = (feature: ChoroplethFeature) => {
    const alpha3 = feature.id == null ? '' : String(feature.id)
    return countryGroups.get(alpha3)
  }

  const getLocalizedName = (alpha3: string, fallback: string) => {
    const alpha2 = alpha3ToAlpha2(alpha3)
    const localized = alpha2 ? countryCodeToName(alpha2, i18n.language) : ''
    return localized || fallback
  }

  const hoveredGroup = hovered ? countryGroups.get(hovered.alpha3) : undefined
  const displayValue = hovered ? (hoveredGroup?.count ?? 0) : totalLocated
  const displayLabel = hovered ? getLocalizedName(hovered.alpha3, hovered.name) : t('widgets.serverMap.total')

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-3">
      <div className="mb-2 flex items-start justify-between gap-2">
        <h3 className="font-semibold text-sm">{t('widgets.serverMap.title')}</h3>
        <div className="flex flex-col items-end text-right">
          <ChartStatFlow
            label={displayLabel}
            labelClassName="max-w-36 truncate text-[10px]"
            value={displayValue}
            valueClassName="text-lg font-semibold leading-none"
          />
        </div>
      </div>

      <div className="flex min-h-0 flex-1 items-center overflow-hidden">
        {isLoading || !worldData ? (
          <div className="flex h-full w-full items-center justify-center text-muted-foreground text-xs">
            {isLoading ? t('states.loading') : t('widgets.serverMap.empty.mapUnavailable')}
          </div>
        ) : (
          <ChoroplethChart aspectRatio="2 / 1" className="max-h-full w-full" data={worldData}>
            <ServerMapHoverBridge onHoverChange={setHovered} />
            <ChoroplethFeatureComponent
              getFeatureColor={(feature: ChoroplethFeature) =>
                countryServerFill(getFeatureGroup(feature)?.count, maxCount)
              }
            />
            <ChoroplethTooltip
              content={({ feature }) => {
                const alpha3 = feature.id == null ? '' : String(feature.id)
                const group = getFeatureGroup(feature)
                const name = getLocalizedName(alpha3, (feature.properties?.name as string | undefined) ?? alpha3)
                return (
                  <TooltipContent rows={[]} title={name}>
                    {group ? (
                      <div className="space-y-0.5">
                        <div className="text-chart-tooltip-muted text-xs">
                          {t('widgets.serverMap.serverCount', { count: group.count })}
                        </div>
                        <div className="max-w-48 text-chart-tooltip-muted text-xs">{group.serverNames.join(', ')}</div>
                      </div>
                    ) : null}
                  </TooltipContent>
                )
              }}
            />
          </ChoroplethChart>
        )}
      </div>

      {countryGroups.size === 0 &&
        (geoStatus?.installed === false ? (
          <div className="space-y-2 py-2 text-center">
            <p className="text-muted-foreground text-xs">{t('widgets.serverMap.empty.noGeoIP')}</p>
            {isAdmin && (
              <Button
                disabled={downloadMutation.isPending}
                onClick={() => downloadMutation.mutate()}
                size="sm"
                variant="outline"
              >
                <Download className="mr-1 size-3.5" />
                {downloadMutation.isPending ? t('states.downloading') : t('widgets.serverMap.actions.downloadGeoIP')}
              </Button>
            )}
          </div>
        ) : (
          <p className="py-2 text-center text-muted-foreground text-xs">
            {t('widgets.serverMap.empty.noLocationData')}
          </p>
        ))}
    </div>
  )
}
