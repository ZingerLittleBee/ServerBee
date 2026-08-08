/**
 * Deterministic capture fixtures for boneyard skeleton generation.
 *
 * Each fixture mirrors the loaded layout of one of the major loading surfaces
 * using static markup and fixed fake data — no queries, no stores, no router
 * navigation, no production data. The same fixture serves two purposes:
 *
 * 1. Build time: `boneyard-js build` renders it on the dev-only
 *    `/boneyard-capture` surface (via the Skeleton `fixture` prop) and
 *    snapshots its geometry into bones.
 * 2. Runtime: while `loading` is true the fixture renders as the Skeleton's
 *    children, hidden with `visibility: hidden`, so the container keeps the
 *    exact loaded-page dimensions and the bone overlay lands without layout
 *    shift.
 *
 * Presentational components that render statically from props (e.g.
 * ServerSummaryCard, StatusBadge) are reused directly; interactive chrome
 * (tabs, toggles, buttons) is replicated with inert markup so fixtures never
 * expose focusable fake controls.
 */
import {
  ArrowDownToLine,
  ArrowLeft,
  ArrowUpFromLine,
  Crown,
  LayoutGrid,
  Pencil,
  Play,
  Server,
  Table2,
  Terminal as TerminalIcon
} from 'lucide-react'
import { CountryFlag } from '@/components/country-flag'
import { StatusBadge } from '@/components/server/status-badge'
import { ServerSummaryCard } from '@/components/status/server-summary-card'
import { ServerSummaryRow } from '@/components/status/server-summary-row'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import type { PublicServerSummary, PublicStatusConfig } from '@/lib/api-schema'
import { cn } from '@/lib/utils'

// ---------------------------------------------------------------------------
// Shared bits
// ---------------------------------------------------------------------------

function BackLinkStub() {
  return (
    <span className="mb-3 inline-flex items-center gap-1 text-muted-foreground text-sm">
      <ArrowLeft aria-hidden="true" className="size-4" />
      Back
    </span>
  )
}

/** Static replica of the detail TabsList — triggers are spans, not buttons. */
function TabListStub({ tabs }: { tabs: string[] }) {
  return (
    <div className="inline-flex h-9 items-center justify-center gap-1 rounded-lg bg-muted p-1 text-muted-foreground">
      {tabs.map((tab, index) => (
        <span
          className={
            index === 0
              ? 'inline-flex items-center justify-center gap-1.5 rounded-md bg-background px-3 py-1 font-medium text-foreground text-sm shadow-sm'
              : 'inline-flex items-center justify-center gap-1.5 rounded-md px-3 py-1 font-medium text-sm'
          }
          key={tab}
        >
          {tab}
        </span>
      ))}
    </div>
  )
}

/** Static replica of one MetricsChart card (title + h-[260px] plot area). */
function ChartCardStub({ title }: { title: string }) {
  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">{title}</h3>
      <div className="h-[260px] w-full" />
    </div>
  )
}

// ---------------------------------------------------------------------------
// /status (public overview)
// ---------------------------------------------------------------------------

const FIXTURE_SERVERS: PublicServerSummary[] = [
  {
    country_code: 'JP',
    group_name: null,
    id: 'fixture-server-1',
    in_maintenance: false,
    metrics: {
      cpu: 32,
      disk_read_bytes_per_sec: 1_048_576,
      disk_total: 84_539_408_384,
      disk_used: 32_212_254_720,
      disk_write_bytes_per_sec: 524_288,
      load_1: 0.42,
      load_5: 0.38,
      load_15: 0.35,
      mem_total: 4_194_304_000,
      mem_used: 1_610_612_736,
      net_in_speed: 102_400,
      net_in_transfer: 12_884_901_888,
      net_out_speed: 51_200,
      net_out_transfer: 4_294_967_296,
      process_count: 128,
      swap_total: 0,
      swap_used: 0,
      tcp_conn: 86,
      udp_conn: 12,
      uptime: 1_209_600
    },
    name: 'tokyo-edge-01',
    online: true,
    os: 'Debian 12',
    public_remark: null,
    region: 'ap-northeast-1',
    uptime_daily: [],
    uptime_percent: 99.99
  },
  {
    country_code: 'DE',
    group_name: null,
    id: 'fixture-server-2',
    in_maintenance: false,
    metrics: {
      cpu: 68,
      disk_read_bytes_per_sec: 2_097_152,
      disk_total: 169_869_581_312,
      disk_used: 96_636_764_160,
      disk_write_bytes_per_sec: 1_048_576,
      load_1: 1.12,
      load_5: 0.98,
      load_15: 0.87,
      mem_total: 8_388_608_000,
      mem_used: 5_966_186_496,
      net_in_speed: 204_800,
      net_in_transfer: 25_769_803_776,
      net_out_speed: 102_400,
      net_out_transfer: 8_589_934_592,
      process_count: 212,
      swap_total: 1_073_741_824,
      swap_used: 134_217_728,
      tcp_conn: 341,
      udp_conn: 28,
      uptime: 5_270_400
    },
    name: 'fra-core-02',
    online: true,
    os: 'Ubuntu 24.04',
    public_remark: 'Main API node',
    region: 'eu-central-1',
    uptime_daily: [],
    uptime_percent: 99.95
  },
  {
    country_code: 'US',
    group_name: null,
    id: 'fixture-server-3',
    in_maintenance: false,
    metrics: null,
    name: 'usw-cache-03',
    online: false,
    os: 'AlmaLinux 9',
    public_remark: null,
    region: 'us-west-2',
    uptime_daily: [],
    uptime_percent: 98.4
  },
  {
    country_code: 'SG',
    group_name: null,
    id: 'fixture-server-4',
    in_maintenance: false,
    metrics: {
      cpu: 12,
      disk_read_bytes_per_sec: 262_144,
      disk_total: 84_539_408_384,
      disk_used: 18_046_741_504,
      disk_write_bytes_per_sec: 131_072,
      load_1: 0.08,
      load_5: 0.1,
      load_15: 0.09,
      mem_total: 2_097_152_000,
      mem_used: 536_870_912,
      net_in_speed: 25_600,
      net_in_transfer: 3_221_225_472,
      net_out_speed: 12_800,
      net_out_transfer: 1_073_741_824,
      process_count: 74,
      swap_total: 0,
      swap_used: 0,
      tcp_conn: 41,
      udp_conn: 6,
      uptime: 604_800
    },
    name: 'sg-relay-04',
    online: true,
    os: 'Debian 12',
    public_remark: null,
    region: 'ap-southeast-1',
    uptime_daily: [],
    uptime_percent: 100
  },
  {
    country_code: 'GB',
    group_name: null,
    id: 'fixture-server-5',
    in_maintenance: false,
    metrics: {
      cpu: 45,
      disk_read_bytes_per_sec: 786_432,
      disk_total: 338_157_633_536,
      disk_used: 150_323_855_360,
      disk_write_bytes_per_sec: 393_216,
      load_1: 0.66,
      load_5: 0.71,
      load_15: 0.69,
      mem_total: 16_777_216_000,
      mem_used: 7_516_192_768,
      net_in_speed: 153_600,
      net_in_transfer: 51_539_607_552,
      net_out_speed: 76_800,
      net_out_transfer: 17_179_869_184,
      process_count: 188,
      swap_total: 2_147_483_648,
      swap_used: 0,
      tcp_conn: 203,
      udp_conn: 19,
      uptime: 2_419_200
    },
    name: 'lon-db-05',
    online: true,
    os: 'Rocky Linux 9',
    public_remark: 'Primary database',
    region: 'eu-west-2',
    uptime_daily: [],
    uptime_percent: 99.9
  },
  {
    country_code: 'AU',
    group_name: null,
    id: 'fixture-server-6',
    in_maintenance: false,
    metrics: {
      cpu: 5,
      disk_read_bytes_per_sec: 131_072,
      disk_total: 42_269_704_192,
      disk_used: 9_007_199_232,
      disk_write_bytes_per_sec: 65_536,
      load_1: 0.03,
      load_5: 0.04,
      load_15: 0.04,
      mem_total: 1_048_576_000,
      mem_used: 402_653_184,
      net_in_speed: 12_800,
      net_in_transfer: 1_610_612_736,
      net_out_speed: 6_400,
      net_out_transfer: 536_870_912,
      process_count: 52,
      swap_total: 0,
      swap_used: 0,
      tcp_conn: 18,
      udp_conn: 3,
      uptime: 86_400
    },
    name: 'syd-mon-06',
    online: true,
    os: 'Alpine 3.20',
    public_remark: null,
    region: 'ap-southeast-2',
    uptime_daily: [],
    uptime_percent: 99.7
  }
]

/** Static replica of LayoutToggle (spans, not a focusable toggle group). */
function LayoutToggleStub({ active }: { active: 'grid' | 'list' }) {
  return (
    <span className="inline-flex items-center gap-1 rounded-md border p-1">
      <span
        className={cn('inline-flex size-8 items-center justify-center rounded-sm', active === 'list' && 'bg-muted')}
      >
        <Table2 aria-hidden="true" className="size-4" />
      </span>
      <span
        className={cn('inline-flex size-8 items-center justify-center rounded-sm', active === 'grid' && 'bg-muted')}
      >
        <LayoutGrid aria-hidden="true" className="size-4" />
      </span>
    </span>
  )
}

// Same fallback thresholds the page uses when the status config has none.
const FIXTURE_UPTIME_THRESHOLDS: Pick<PublicStatusConfig, 'uptime_red_threshold' | 'uptime_yellow_threshold'> = {
  uptime_red_threshold: 95,
  uptime_yellow_threshold: 100
}

/** Mirrors the loaded grid layout of `status.index.tsx`. */
export function StatusOverviewGridFixture() {
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-end">
        <LayoutToggleStub active="grid" />
      </div>
      <div className="grid gap-4" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))' }}>
        {FIXTURE_SERVERS.map((server) => (
          <ServerSummaryCard clickable={false} key={server.id} server={server} />
        ))}
      </div>
    </div>
  )
}

/** Mirrors the loaded list (table) layout of `status.index.tsx`. */
export function StatusOverviewListFixture() {
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-end">
        <LayoutToggleStub active="list" />
      </div>
      <div className="overflow-hidden rounded-md border">
        <Table className="min-w-[1120px]">
          <TableHeader>
            <TableRow>
              <TableHead className="min-w-[220px]">Servers</TableHead>
              <TableHead className="w-[180px]">CPU</TableHead>
              <TableHead className="w-[180px]">Memory</TableHead>
              <TableHead className="w-[184px]">Disk</TableHead>
              <TableHead className="hidden w-[184px] lg:table-cell">Network In</TableHead>
              <TableHead className="hidden w-[220px] xl:table-cell">Uptime</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {FIXTURE_SERVERS.map((server) => (
              <ServerSummaryRow
                clickable={false}
                key={server.id}
                server={server}
                thresholds={FIXTURE_UPTIME_THRESHOLDS}
              />
            ))}
          </TableBody>
        </Table>
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Server detail (public /status/server/$serverId and authed /servers/$id)
// ---------------------------------------------------------------------------

const DETAIL_META_PUBLIC = ['Debian 12', 'AMD EPYC 7702P (4 cores) x86_64', '16 GB', '5.15.0-91-generic', 'ap-northeast-1']

const DETAIL_META_ADMIN = [
  'Ubuntu 24.04',
  'AMD EPYC 7702P (4 cores) x86_64',
  '16 GB',
  '203.0.113.10',
  '2001:db8::10',
  '6.8.0-45-generic',
  'eu-central-1'
]

const RANGE_LABELS_PUBLIC = ['1h', '6h', '24h', '7d', '30d']
const RANGE_LABELS_ADMIN = ['Realtime', '1h', '6h', '24h', '7d', '30d']

const CHART_TITLES = ['CPU', 'Memory', 'Disk', 'Network In', 'Network Out', 'Load']

function RangeButtonsStub({ labels }: { labels: string[] }) {
  return (
    <div className="flex flex-wrap gap-1">
      {labels.map((label, index) => (
        <Button disabled key={label} size="sm" variant={index === 0 ? 'default' : 'outline'}>
          {label}
        </Button>
      ))}
    </div>
  )
}

/**
 * Mirrors the loaded server detail layout shared by the public status detail
 * and the authed detail page (both render `ServerDetailContent` under a
 * back-link + title + meta header). `variant="admin"` adds the action row,
 * agent version card, and the wider admin tab set.
 */
export function ServerDetailFixture({ variant }: { variant: 'admin' | 'public' }) {
  const isAdmin = variant === 'admin'
  const meta = isAdmin ? DETAIL_META_ADMIN : DETAIL_META_PUBLIC
  const tabs = isAdmin ? ['Metrics', 'Network', 'Security', 'IP Quality'] : ['Metrics', 'Network']
  const rangeLabels = isAdmin ? RANGE_LABELS_ADMIN : RANGE_LABELS_PUBLIC

  return (
    <div>
      <div className="mb-6">
        <BackLinkStub />
        {isAdmin ? (
          <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-start">
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-3">
                <CountryFlag className="text-xl" code="JP" />
                <h1 className="font-bold text-2xl">fra-core-02</h1>
                <StatusBadge status="online" />
              </div>
              <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground text-sm">
                {meta.map((item) => (
                  <span key={item}>{item}</span>
                ))}
              </div>
            </div>
            <div className="sm:justify-self-end">
              <div className="flex flex-wrap gap-2">
                <Button disabled size="sm" variant="outline">
                  <Pencil aria-hidden="true" className="mr-1 size-4" />
                  Edit
                </Button>
                <Button disabled size="sm" variant="outline">
                  Capabilities
                </Button>
                <Button disabled size="sm" variant="outline">
                  <TerminalIcon aria-hidden="true" className="mr-1 size-4" />
                  Terminal
                </Button>
              </div>
            </div>
            <div className="sm:col-span-2">
              <Card>
                <CardHeader>
                  <CardTitle>Agent upgrade</CardTitle>
                  <CardDescription>Current agent version</CardDescription>
                </CardHeader>
                <CardContent>
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-lg">v1.0.0</span>
                      <Badge variant="secondary">Latest: v1.1.0</Badge>
                    </div>
                    <Button disabled size="sm">
                      Upgrade
                    </Button>
                  </div>
                </CardContent>
              </Card>
            </div>
          </div>
        ) : (
          <>
            <div className="flex items-center gap-3">
              <CountryFlag className="text-xl" code="JP" />
              <h1 className="font-bold text-2xl">tokyo-edge-01</h1>
              <StatusBadge status="online" />
            </div>
            <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground text-sm">
              {meta.map((item) => (
                <span key={item}>{item}</span>
              ))}
            </div>
          </>
        )}
      </div>

      <TabListStub tabs={tabs} />

      <div className="mt-4">
        <div className="mb-6 flex flex-wrap gap-6 rounded-xl bg-card p-3 text-sm ring-1 ring-foreground/10">
          <span className="text-muted-foreground">Network In 1.2 MB/s</span>
          <span className="text-muted-foreground">Network Out 0.6 MB/s</span>
          <span className="text-muted-foreground">Total 128 GB</span>
        </div>
        <div className="mb-6 rounded-xl bg-card p-4 ring-1 ring-foreground/10">
          <div className="mb-3 flex items-center justify-between">
            <h3 className="font-semibold text-sm">Uptime</h3>
            <span className="font-medium text-sm">99.97%</span>
          </div>
          <div className="h-12 w-full rounded-md bg-muted/40" />
        </div>
      </div>

      <div className="mt-4 space-y-4">
        <RangeButtonsStub labels={rangeLabels} />
        <div className="grid gap-4 lg:grid-cols-2">
          {CHART_TITLES.map((title) => (
            <ChartCardStub key={title} title={title} />
          ))}
        </div>
        {isAdmin && (
          <Card>
            <CardHeader>
              <CardTitle>Traffic</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="h-2 w-full rounded-full bg-muted" />
              <div className="mt-2 flex justify-between text-muted-foreground text-xs">
                <span>128 GB / 1 TB</span>
                <span>12d left</span>
              </div>
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// /status/network/$serverId (standalone public network detail)
// ---------------------------------------------------------------------------

const FIXTURE_TARGETS = [
  { id: 'fixture-target-1', latency: '12.3 ms', loss: '0%', name: 'Telecom · Shanghai' },
  { id: 'fixture-target-2', latency: '18.7 ms', loss: '0%', name: 'Unicom · Beijing' },
  { id: 'fixture-target-3', latency: '21.4 ms', loss: '0.5%', name: 'Mobile · Guangzhou' },
  { id: 'fixture-target-4', latency: '148.2 ms', loss: '0%', name: 'International · Frankfurt' },
  { id: 'fixture-target-5', latency: '162.9 ms', loss: '1%', name: 'International · San Jose' },
  { id: 'fixture-target-6', latency: '96.5 ms', loss: '0%', name: 'International · Singapore' }
]

/** Mirrors the loaded layout of `status.network.$serverId.tsx`. */
export function StatusNetworkFixture() {
  return (
    <div>
      <div className="mb-6">
        <BackLinkStub />
        <div className="flex items-center gap-3">
          <h1 className="font-bold text-2xl">tokyo-edge-01</h1>
          <StatusBadge status="online" />
        </div>
        <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground text-sm">
          <span>Last probe: Aug 8, 14:32</span>
        </div>
      </div>

      <div className="mb-4">
        <TabListStub tabs={['All targets', 'By provider']} />
        <div className="flex flex-wrap gap-2 pt-2">
          {FIXTURE_TARGETS.map((target) => (
            <div
              className="flex min-w-[160px] items-center gap-3 rounded-lg border bg-card px-3 py-2"
              key={target.id}
            >
              <div aria-hidden="true" className="size-3 shrink-0 rounded-full bg-muted" />
              <div className="min-w-0 flex-1">
                <p className="truncate font-medium text-sm">{target.name}</p>
                <div className="flex items-center gap-2 text-muted-foreground text-xs">
                  <span className="font-mono">{target.latency}</span>
                  <span className="text-muted-foreground/60">|</span>
                  <span>Loss {target.loss}</span>
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// /traffic (authed traffic overview)
// ---------------------------------------------------------------------------

function TrafficStatCardStub({ label, value }: { label: string; value: string }) {
  return (
    <Card>
      <CardContent className="flex items-center gap-4">
        <div className="flex size-10 items-center justify-center rounded-lg bg-muted">
          <Server aria-hidden="true" className="size-5 text-muted-foreground" />
        </div>
        <div>
          <p className="text-muted-foreground text-sm">{label}</p>
          <p className="font-semibold text-lg">{value}</p>
        </div>
      </CardContent>
    </Card>
  )
}

const FIXTURE_TRAFFIC_ROWS = [
  { days: '12d', in: '512 GB', limit: '1 TB', name: 'fra-core-02', out: '128 GB', total: '640 GB', usage: '62.5%' },
  { days: '20d', in: '256 GB', limit: '512 GB', name: 'tokyo-edge-01', out: '64 GB', total: '320 GB', usage: '62.5%' },
  { days: '8d', in: '128 GB', limit: '200 GB', name: 'lon-db-05', out: '32 GB', total: '160 GB', usage: '80.0%' },
  { days: '-', in: '64 GB', limit: 'Unlimited', name: 'sg-relay-04', out: '16 GB', total: '80 GB', usage: 'N/A' },
  { days: '25d', in: '32 GB', limit: '100 GB', name: 'syd-mon-06', out: '8 GB', total: '40 GB', usage: '40.0%' }
]

/** Mirrors the loaded layout of `_authed/traffic/index.tsx`. */
export function TrafficOverviewFixture() {
  return (
    <div className="w-full min-w-0 max-w-[calc(100vw-1.5rem)] overflow-hidden sm:max-w-full">
      <h1 className="mb-6 font-bold text-2xl">Traffic Overview</h1>

      <div className="mb-6 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <TrafficStatCardStub label="Cycle Inbound" value="992 GB" />
        <TrafficStatCardStub label="Cycle Outbound" value="248 GB" />
        <TrafficStatCardStub label="Highest Usage" value="fra-core-02" />
        <TrafficStatCardStub label="Servers Over 80%" value="1" />
      </div>

      <div className="mb-6 min-w-0 max-w-full overflow-x-auto rounded-lg border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Server</TableHead>
              <TableHead>Inbound</TableHead>
              <TableHead>Outbound</TableHead>
              <TableHead>Total</TableHead>
              <TableHead>Limit</TableHead>
              <TableHead>Usage</TableHead>
              <TableHead>Days Left</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {FIXTURE_TRAFFIC_ROWS.map((row) => (
              <TableRow key={row.name}>
                <TableCell className="font-medium">{row.name}</TableCell>
                <TableCell className="tabular-nums">{row.in}</TableCell>
                <TableCell className="tabular-nums">{row.out}</TableCell>
                <TableCell className="tabular-nums">{row.total}</TableCell>
                <TableCell className="tabular-nums">{row.limit}</TableCell>
                <TableCell>
                  <div className="flex items-center gap-2">
                    <div className="h-2 w-24 shrink-0 overflow-hidden rounded-full bg-muted" />
                    <span className="text-xs tabular-nums">{row.usage}</span>
                  </div>
                </TableCell>
                <TableCell className="tabular-nums">{row.days}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Global 30-Day Trend</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="h-[300px] w-full" />
        </CardContent>
      </Card>
    </div>
  )
}

// ---------------------------------------------------------------------------
// /service-monitors/$id (authed service monitor detail)
// ---------------------------------------------------------------------------

const FIXTURE_MONITOR_HISTORY = [
  { error: '--', id: 'fixture-record-1', latency: '182.4 ms', ok: true, time: 'Aug 8, 14:30' },
  { error: '--', id: 'fixture-record-2', latency: '175.1 ms', ok: true, time: 'Aug 8, 14:29' },
  { error: 'TLS handshake failed', id: 'fixture-record-3', latency: '--', ok: false, time: 'Aug 8, 14:28' },
  { error: '--', id: 'fixture-record-4', latency: '190.2 ms', ok: true, time: 'Aug 8, 14:27' },
  { error: '--', id: 'fixture-record-5', latency: '181.8 ms', ok: true, time: 'Aug 8, 14:26' },
  { error: '--', id: 'fixture-record-6', latency: '177.6 ms', ok: true, time: 'Aug 8, 14:25' }
]

function MonitorStatCardStub({ label, value }: { label: string; value: string }) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="text-muted-foreground text-xs">{label}</CardTitle>
      </CardHeader>
      <CardContent>
        <p className="font-bold text-2xl">{value}</p>
      </CardContent>
    </Card>
  )
}

/** Mirrors the loaded layout of `_authed/service-monitors/$id.tsx`. */
export function ServiceMonitorDetailFixture() {
  return (
    <div>
      <div className="mb-6">
        <BackLinkStub />
        <div className="flex items-start justify-between">
          <div>
            <div className="flex items-center gap-3">
              <h1 className="font-bold text-2xl">shop.example.com TLS</h1>
              <Badge variant="secondary">SSL</Badge>
              <Badge variant="outline">Online</Badge>
            </div>
            <p className="mt-1 font-mono text-muted-foreground text-sm">example.com:443</p>
          </div>
          <div className="flex gap-2">
            <Button disabled size="sm" variant="outline">
              <Play aria-hidden="true" className="size-4" />
              Check now
            </Button>
          </div>
        </div>
      </div>

      <div className="space-y-6">
        <div className="grid gap-4 sm:grid-cols-3">
          <MonitorStatCardStub label="Uptime" value="99.2%" />
          <MonitorStatCardStub label="Avg Latency" value="181.4 ms" />
          <MonitorStatCardStub label="Last Check" value="Aug 8, 14:30" />
        </div>

        <div className="rounded-lg border bg-card p-4">
          <h3 className="mb-3 font-semibold text-sm">Response Time</h3>
          <div className="h-[260px] w-full" />
        </div>

        <Card>
          <CardHeader>
            <CardTitle>SSL Certificate</CardTitle>
          </CardHeader>
          <CardContent>
            <dl className="grid gap-2 text-sm sm:grid-cols-2">
              <div>
                <dt className="text-muted-foreground text-xs">Subject</dt>
                <dd className="break-all font-mono text-sm">CN=example.com</dd>
              </div>
              <div>
                <dt className="text-muted-foreground text-xs">Issuer</dt>
                <dd className="break-all font-mono text-sm">CN=R3, O=Let&apos;s Encrypt</dd>
              </div>
              <div>
                <dt className="text-muted-foreground text-xs">Expires</dt>
                <dd className="break-all font-mono text-sm">Nov 6, 2026</dd>
              </div>
              <div>
                <dt className="text-muted-foreground text-xs">Days Remaining</dt>
                <dd className="break-all font-mono text-sm">90</dd>
              </div>
            </dl>
          </CardContent>
        </Card>

        <div>
          <h3 className="mb-3 font-semibold text-lg">History</h3>
          <div className="rounded-lg border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Time</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Latency</TableHead>
                  <TableHead>Error</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {FIXTURE_MONITOR_HISTORY.map((record) => (
                  <TableRow key={record.id}>
                    <TableCell className="text-xs">{record.time}</TableCell>
                    <TableCell>
                      <span className="inline-flex items-center gap-1 text-xs">
                        <span className="inline-block size-2 rounded-full bg-muted" />
                        {record.ok ? 'OK' : 'Fail'}
                      </span>
                    </TableCell>
                    <TableCell className="font-mono text-xs">{record.latency}</TableCell>
                    <TableCell className="max-w-[300px] truncate text-muted-foreground text-xs">{record.error}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </div>
      </div>
    </div>
  )
}
