import { ShieldCheck } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { IpQualityCard } from '@/components/ip-quality/ip-quality-card'
import { UnlockMatrix } from '@/components/ip-quality/unlock-matrix'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import type { PublicIpQualityEntry, PublicIpQualityOverview, PublicIpQualityServiceMeta } from '@/lib/api-schema'
import type { ServerIpQualityData, UnlockService } from '@/lib/ip-quality-types'

interface ServerLite {
  id: string
  name: string
}

interface AdminProps {
  isLoading: boolean
  overview: ServerIpQualityData[]
  servers: ServerLite[]
  services: UnlockService[]
  variant: 'admin'
}

interface PublicProps {
  data: PublicIpQualityOverview
  isLoading: boolean
  /**
   * Display names for servers. The public DTO carries only `server_id`; the
   * caller is expected to derive labels from the public servers feed. When a
   * name isn't available, the id is rendered as a fallback.
   */
  serverNames: Map<string, string>
  variant: 'public'
}

export type IpQualityContentProps = AdminProps | PublicProps

export function IpQualityContent(props: IpQualityContentProps) {
  const { t } = useTranslation('ip-quality')

  if (props.variant === 'admin') {
    return <AdminBody {...props} />
  }
  return <PublicBody {...props} t={t} />
}

function AdminBody({ overview, services, servers, isLoading }: AdminProps) {
  const { t } = useTranslation('ip-quality')

  // Only services that are enabled show in the matrix
  const enabledServices = useMemo(() => services.filter((s) => s.enabled), [services])

  // Servers that have any IP quality data (appear in overview), ordered by server name
  const serversWithData = useMemo(() => {
    const overviewIds = new Set(overview.map((o) => o.server_id))
    return servers.filter((s) => overviewIds.has(s.id)).sort((a, b) => a.name.localeCompare(b.name))
  }, [servers, overview])

  const overviewByServerId = useMemo(() => new Map(overview.map((o) => [o.server_id, o])), [overview])

  const hasServers = servers.length > 0
  const hasData = serversWithData.length > 0

  return (
    <ScrollArea className="h-full w-full" contentClassName="min-w-0!">
      <div className="space-y-6 pr-1 pb-4">
        <div>
          <h1 className="font-bold text-2xl">{t('page_title')}</h1>
          <p className="text-muted-foreground text-sm">{t('page_description')}</p>
        </div>

        {isLoading && (
          <div className="space-y-3">
            {Array.from({ length: 3 }, (_, i) => (
              <Skeleton className="h-28 rounded-xl" key={`skel-${i.toString()}`} />
            ))}
          </div>
        )}

        {!(isLoading || hasServers) && (
          <div className="flex min-h-[240px] items-center justify-center rounded-xl border border-dashed">
            <div className="space-y-2 text-center">
              <ShieldCheck aria-hidden="true" className="mx-auto size-8 text-muted-foreground" />
              <p className="text-muted-foreground text-sm">{t('no_servers')}</p>
            </div>
          </div>
        )}

        {!isLoading && hasServers && !hasData && (
          <div className="flex min-h-[240px] items-center justify-center rounded-xl border border-dashed">
            <div className="space-y-2 text-center">
              <ShieldCheck aria-hidden="true" className="mx-auto size-8 text-muted-foreground" />
              <p className="font-medium text-sm">{t('no_data')}</p>
              <p className="max-w-xs text-muted-foreground text-xs">
                {t('no_data_overview_hint', { cap: 'ip_quality', flag: '--allow-cap ip_quality' })}
              </p>
            </div>
          </div>
        )}

        {!isLoading && hasData && (
          <>
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
              {serversWithData.map((server) => {
                const data = overviewByServerId.get(server.id)
                return <IpQualityCard ipQuality={data?.ip_quality ?? null} key={server.id} serverName={server.name} />
              })}
            </div>

            {enabledServices.length > 0 && (
              <div className="space-y-2">
                <h2 className="font-semibold text-base">{t('unlock_matrix')}</h2>
                <UnlockMatrix overview={overview} servers={serversWithData} services={enabledServices} />
              </div>
            )}
          </>
        )}
      </div>
    </ScrollArea>
  )
}

function PublicBody({
  data,
  serverNames,
  isLoading,
  t
}: PublicProps & { t: ReturnType<typeof useTranslation<'ip-quality'>>['t'] }) {
  const entries: PublicIpQualityEntry[] = data.entries
  const services: PublicIpQualityServiceMeta[] = data.services

  const serversWithData = useMemo(() => {
    return entries
      .map((e) => ({ id: e.server_id, name: serverNames.get(e.server_id) ?? e.server_id }))
      .sort((a, b) => a.name.localeCompare(b.name))
  }, [entries, serverNames])

  const entryByServerId = useMemo(() => new Map(entries.map((e) => [e.server_id, e])), [entries])

  const hasData = serversWithData.length > 0

  return (
    <div className="space-y-6">
      <div>
        <h1 className="font-bold text-2xl">{t('page_title')}</h1>
        <p className="text-muted-foreground text-sm">{t('page_description')}</p>
      </div>

      {isLoading && (
        <div className="space-y-3">
          {Array.from({ length: 3 }, (_, i) => (
            <Skeleton className="h-28 rounded-xl" key={`skel-${i.toString()}`} />
          ))}
        </div>
      )}

      {!(isLoading || hasData) && (
        <div className="flex min-h-[240px] items-center justify-center rounded-xl border border-dashed">
          <div className="space-y-2 text-center">
            <ShieldCheck aria-hidden="true" className="mx-auto size-8 text-muted-foreground" />
            <p className="font-medium text-sm">{t('no_data')}</p>
          </div>
        </div>
      )}

      {!isLoading && hasData && (
        <>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {serversWithData.map((server) => {
              const entry = entryByServerId.get(server.id)
              return (
                <IpQualityCard
                  ipQuality={entry?.ip_quality ?? null}
                  key={server.id}
                  serverName={server.name}
                  variant="public"
                />
              )
            })}
          </div>

          {services.length > 0 && (
            <div className="space-y-2">
              <h2 className="font-semibold text-base">{t('unlock_matrix')}</h2>
              <UnlockMatrix overview={entries} servers={serversWithData} services={services} />
            </div>
          )}
        </>
      )}
    </div>
  )
}
