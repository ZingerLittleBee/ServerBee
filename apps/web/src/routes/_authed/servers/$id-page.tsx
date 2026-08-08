import { useQuery } from '@tanstack/react-query'
import { getRouteApi, Link } from '@tanstack/react-router'
import { ArrowLeft, Container, FileText, Pencil, Terminal as TerminalIcon } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { CountryFlag } from '@/components/country-flag'
import { NetworkTab } from '@/components/network/network-tab'
import { ServerDetailSkeleton } from '@/components/boneyard/page-skeletons'
import { AgentReenrollmentDialog } from '@/components/server/agent-reenrollment-dialog'
import { AgentVersionSection } from '@/components/server/agent-version-section'
import { CapabilitiesDialog } from '@/components/server/capabilities-dialog'
import { ServerEditDialog } from '@/components/server/server-edit-dialog'
import { StatusBadge } from '@/components/server/status-badge'
import { UpgradeJobBadge } from '@/components/server/upgrade-job-badge'
import { ServerDetailContent } from '@/components/status/server-detail-content'
import { Button } from '@/components/ui/button'
import { useServer } from '@/hooks/use-api'
import { useAuth } from '@/hooks/use-auth'
import { api } from '@/lib/api-client'
import type { ServerResponse } from '@/lib/api-schema'
import { CAP_DOCKER, CAP_FILE, CAP_TERMINAL, getEffectiveCapabilityEnabled } from '@/lib/capabilities'
import { useLiveServers } from '@/lib/server-catalog'
import type { ServerDetailTab } from '@/lib/server-detail-nav'
import { formatBytes } from '@/lib/utils'
import { useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'

const routeApi = getRouteApi('/_authed/servers/$id')

interface ServerWithCaps {
  agent_local_capabilities?: number | null
  capabilities?: number | null
  effective_capabilities?: number | null
  id: string
  protocol_version?: number | null
}

function ServerInfoMeta({ server }: { server: ServerResponse }) {
  const { t } = useTranslation('servers')
  return (
    <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground text-sm">
      {server.os && (
        <span>
          {t('detail_os')} {server.os}
        </span>
      )}
      {server.cpu_name && (
        <span>
          {t('detail_cpu')} {server.cpu_name}
          {server.cpu_cores != null && ` (${t('detail_cores', { count: server.cpu_cores })})`}
          {server.cpu_arch && ` ${server.cpu_arch}`}
        </span>
      )}
      {server.mem_total != null && (
        <span>
          {t('detail_ram')} {formatBytes(server.mem_total)}
        </span>
      )}
      {server.ipv4 && (
        <span>
          {t('detail_ipv4_label')} {server.ipv4}
        </span>
      )}
      {server.ipv6 && (
        <span>
          {t('detail_ipv6_label')} {server.ipv6}
        </span>
      )}
      {server.kernel_version && (
        <span>
          {t('detail_kernel_label')} {server.kernel_version}
        </span>
      )}
      {server.region && (
        <span>
          {t('detail_region_label')} {server.region}
        </span>
      )}
      {server.agent_version && <span>{t('detail_agent_label', { version: server.agent_version })}</span>}
    </div>
  )
}

function ServerActionButtons({
  dockerEnabled,
  fileEnabled,
  id,
  isAdmin,
  isOnline,
  liveHydrated,
  onEditOpen,
  onReenrollmentOpen,
  serverWithCaps,
  terminalEnabled
}: {
  dockerEnabled: boolean
  fileEnabled: boolean
  id: string
  isAdmin: boolean
  isOnline: boolean
  liveHydrated: boolean
  onEditOpen: () => void
  onReenrollmentOpen: () => void
  serverWithCaps: ServerResponse & ServerWithCaps
  terminalEnabled: boolean
}) {
  const { t } = useTranslation('servers')
  // Gate online/offline-specific buttons on liveHydrated so the button list does
  // not flicker while online-only Terminal/Files/Docker actions hydrate.
  return (
    <div className="flex flex-wrap gap-2">
      <Button onClick={onEditOpen} size="sm" variant="outline">
        <Pencil aria-hidden="true" className="mr-1 size-4" />
        {t('detail_edit')}
      </Button>
      <CapabilitiesDialog server={serverWithCaps} />
      {isAdmin && liveHydrated && (
        <Button onClick={onReenrollmentOpen} size="sm" variant="outline">
          {t('detail_agent_reenrollment', { defaultValue: 'Agent re-enrollment' })}
        </Button>
      )}
      {liveHydrated && isOnline && terminalEnabled && (
        <Link params={{ serverId: id }} to="/terminal/$serverId">
          <Button size="sm" variant="outline">
            <TerminalIcon aria-hidden="true" className="mr-1 size-4" />
            {t('detail_terminal')}
          </Button>
        </Link>
      )}
      {liveHydrated && isOnline && fileEnabled && (
        <Link params={{ serverId: id }} search={{ path: '/' }} to="/files/$serverId">
          <Button size="sm" variant="outline">
            <FileText aria-hidden="true" className="mr-1 size-4" />
            {t('detail_files')}
          </Button>
        </Link>
      )}
      {liveHydrated && isOnline && dockerEnabled && (
        <Link params={{ serverId: id }} to="/servers/$serverId/docker">
          <Button size="sm" variant="outline">
            <Container aria-hidden="true" className="mr-1 size-4" />
            {t('detail_docker')}
          </Button>
        </Link>
      )}
    </div>
  )
}

export function ServerDetailPage() {
  const { t } = useTranslation('servers')
  const { id } = routeApi.useParams()
  const { range: rangeParam, tab: tabParam } = routeApi.useSearch()
  const navigate = routeApi.useNavigate()
  const [editOpen, setEditOpen] = useState(false)
  const [reenrollmentOpen, setReenrollmentOpen] = useState(false)
  const { user } = useAuth()
  const { data: latestAgentVersion } = useQuery<{ version?: string | null }>({
    queryKey: ['agent', 'latest-version'],
    queryFn: () => api.get<{ version?: string | null }>('/api/agent/latest-version'),
    staleTime: 60_000
  })

  const { data: server, isLoading: serverLoading } = useServer(id)

  const { data: liveServers } = useLiveServers()
  const liveHydrated = liveServers !== undefined
  const liveData = liveServers?.find((s) => s.id === id)
  const upgradeJob = useUpgradeJobsStore((state) => state.jobs.get(id))
  const isAdmin = user?.role === 'admin'

  if (serverLoading) {
    return <ServerDetailSkeleton />
  }

  if (!server) {
    return (
      <div className="flex min-h-[400px] items-center justify-center">
        <p className="text-muted-foreground">{t('detail_not_found')}</p>
      </div>
    )
  }

  const serverWithCaps = server as ServerResponse & ServerWithCaps
  const isOnline = liveData?.online ?? false
  const terminalEnabled = getEffectiveCapabilityEnabled(
    serverWithCaps.effective_capabilities,
    serverWithCaps.capabilities,
    CAP_TERMINAL
  )
  const fileEnabled = getEffectiveCapabilityEnabled(
    serverWithCaps.effective_capabilities,
    serverWithCaps.capabilities,
    CAP_FILE
  )
  const dockerEnabled = getEffectiveCapabilityEnabled(
    serverWithCaps.effective_capabilities,
    serverWithCaps.capabilities,
    CAP_DOCKER
  )

  return (
    <div className="pb-6">
      <div className="mb-6">
        <Link
          className="mb-3 inline-flex items-center gap-1 text-muted-foreground text-sm hover:text-foreground"
          to="/"
        >
          <ArrowLeft aria-hidden="true" className="size-4" />
          {t('detail_back')}
        </Link>

        <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-start">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-3">
              <CountryFlag className="text-xl" code={server.country_code} />
              <h1 className="font-bold text-2xl">{server.name}</h1>
              <StatusBadge status={isOnline ? 'online' : 'offline'} />
              <UpgradeJobBadge job={upgradeJob} />
            </div>
            <ServerInfoMeta server={server} />
          </div>
          {/* DOM order matches the rendered order: the actions sit beside the
              title on sm+, so they must follow it in the tab sequence. The
              version section spans both columns and falls to the second row. */}
          <div className="sm:justify-self-end">
            <ServerActionButtons
              dockerEnabled={dockerEnabled}
              fileEnabled={fileEnabled}
              id={id}
              isAdmin={isAdmin}
              isOnline={isOnline}
              liveHydrated={liveHydrated}
              onEditOpen={() => setEditOpen(true)}
              onReenrollmentOpen={() => setReenrollmentOpen(true)}
              serverWithCaps={serverWithCaps}
              terminalEnabled={terminalEnabled}
            />
          </div>
          <div className="sm:col-span-2">
            <AgentVersionSection
              agentVersion={server.agent_version}
              configuredCapabilities={serverWithCaps.capabilities}
              effectiveCapabilities={serverWithCaps.effective_capabilities}
              latestVersion={latestAgentVersion?.version ?? null}
              serverId={id}
            />
          </div>
        </div>
      </div>

      <ServerDetailContent
        activeTab={tabParam ?? 'metrics'}
        networkTab={
          <NetworkTab
            onRangeChange={(rangeKey) => navigate({ search: (prev) => ({ ...prev, range: rangeKey }) })}
            rangeKey={rangeParam}
            serverId={id}
          />
        }
        onRangeChange={(rangeKey) => navigate({ search: (prev) => ({ ...prev, range: rangeKey }) })}
        onTabChange={(tab) => navigate({ search: (prev) => ({ ...prev, tab: tab as ServerDetailTab }) })}
        rangeKey={rangeParam}
        server={server}
        serverId={id}
        variant="admin"
      />

      <ServerEditDialog onClose={() => setEditOpen(false)} open={editOpen} server={server} />
      <AgentReenrollmentDialog
        onOpenChange={setReenrollmentOpen}
        open={reenrollmentOpen}
        server={{
          id: server.id,
          name: server.name,
          capabilities: server.capabilities,
          agent_authority: server.agent_authority
        }}
      />
    </div>
  )
}
