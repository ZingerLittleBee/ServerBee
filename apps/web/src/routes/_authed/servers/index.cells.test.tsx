import { render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { TrafficOverviewItem } from '@/hooks/use-traffic-overview'
import { CpuCell, DiskCell, MemoryCell, MetricBarRow, NameCell, NetworkCell, UptimeCell } from './index.cells'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key })
}))

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, ...props }: { children?: React.ReactNode; [k: string]: unknown }) => (
    <a data-testid="server-link" href={`/servers/${props.params && (props.params as { id: string }).id}`}>
      {children}
    </a>
  )
}))

const REGEX_BG_EMERALD = /bg-emerald-500/
const REGEX_BG_AMBER = /bg-amber-500/
const REGEX_BG_RED = /bg-red-500/
const REGEX_CPU_CORES_LOAD = /8 cores · load 1\.23/
const REGEX_CORES = /cores/
const REGEX_LOAD_1_23 = /load 1\.23/
const REGEX_LOAD = /load/
const REGEX_MEM_USED_TOTAL = /7\.2 GB \/ 16\.0 GB/
const REGEX_SWAP = /swap/
const REGEX_3_PCT = /3%/
const REGEX_SWAP_0_PCT = /swap 0%/
const REGEX_DISK_READ = /2\.0 MB\/s/
const REGEX_DISK_WRITE = /500\.0 KB\/s/
const REGEX_DISK_USED_TOTAL = /55\.9 GB \/ 93\.1 GB/
const REGEX_DISK_ZERO = /0 B \/ 0 B/
const REGEX_KB_PER_SEC = /KB\/s/
const REGEX_MB_PER_SEC = /MB\/s/
const REGEX_TRAFFIC_USED_LIMIT = /93\.2 GB \/ 1\.0 TB/
const REGEX_TRAFFIC_DOWN = /1\.1 MB\/s/
const REGEX_TRAFFIC_UP = /332\.0 KB\/s/
const REGEX_TRAFFIC_FALLBACK = /3\.0 GB \/ 1\.0 TB/
const REGEX_TRAFFIC_OFFLINE_USAGE = /100\.0 GB \/ 1\.0 TB/
const REGEX_LIMIT_DEFAULT = /1\.0 TB/
const REGEX_UPTIME_23D = /23d/
const REGEX_OS_UBUNTU = /Ubuntu 22\.04/
const REGEX_OFFLINE = /offline/i
const REGEX_LAST_SEEN = /last_seen_ago/

function makeServer(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 'srv-1',
    name: 'test-server',
    online: true,
    country_code: null,
    cpu: 0,
    cpu_cores: null,
    cpu_name: null,
    disk_read_bytes_per_sec: 0,
    disk_total: 500_000_000_000,
    disk_used: 120_000_000_000,
    disk_write_bytes_per_sec: 0,
    features: [],
    group_id: null,
    last_active: 0,
    load1: 0,
    load5: 0,
    load15: 0,
    mem_total: 8_000_000_000,
    mem_used: 3_200_000_000,
    net_in_speed: 0,
    net_in_transfer: 0,
    net_out_speed: 0,
    net_out_transfer: 0,
    os: null,
    process_count: 0,
    region: null,
    swap_total: 0,
    swap_used: 0,
    tags: [],
    tcp_conn: 0,
    udp_conn: 0,
    uptime: 0,
    ...overrides
  }
}

describe('MetricBarRow', () => {
  it('renders green bar below 70%', () => {
    const { container } = render(<MetricBarRow icon={null} pct={50} />)
    const fill = container.querySelector('[data-slot="metric-bar-fill"]')
    expect(fill?.className).toMatch(REGEX_BG_EMERALD)
  })

  it('renders amber bar at 70% and below 90%', () => {
    const { container } = render(<MetricBarRow icon={null} pct={70.5} />)
    const fill = container.querySelector('[data-slot="metric-bar-fill"]')
    expect(fill?.className).toMatch(REGEX_BG_AMBER)
  })

  it('renders red bar at 90%+', () => {
    const { container } = render(<MetricBarRow icon={null} pct={92} />)
    const fill = container.querySelector('[data-slot="metric-bar-fill"]')
    expect(fill?.className).toMatch(REGEX_BG_RED)
  })

  it('rounds the percentage to 0 decimals', () => {
    render(<MetricBarRow icon={null} pct={42.67} />)
    expect(screen.getByText('43%')).toBeDefined()
  })

  it('clamps percentage to [0, 100]', () => {
    render(<MetricBarRow icon={null} pct={150} />)
    expect(screen.getByText('100%')).toBeDefined()
    render(<MetricBarRow icon={null} pct={-5} />)
    expect(screen.getByText('0%')).toBeDefined()
  })

  it('renders the supplied icon slot', () => {
    render(<MetricBarRow icon={<span data-testid="cpu-icon" />} pct={10} />)
    expect(screen.getByTestId('cpu-icon')).toBeDefined()
  })
})

describe('CpuCell', () => {
  it('renders cores + load when cpu_cores is present', () => {
    render(<CpuCell server={makeServer({ cpu: 12, cpu_cores: 8, load1: 1.234 })} />)
    expect(screen.getByText('12%')).toBeDefined()
    expect(screen.getByText(REGEX_CPU_CORES_LOAD)).toBeDefined()
  })

  it('falls back to load-only when cpu_cores is null (Phase A)', () => {
    render(<CpuCell server={makeServer({ cpu: 12, cpu_cores: null, load1: 1.23 })} />)
    expect(screen.queryByText(REGEX_CORES)).toBeNull()
    expect(screen.getByText(REGEX_LOAD_1_23)).toBeDefined()
  })

  it('hides sub-line when offline', () => {
    render(<CpuCell server={makeServer({ online: false, cpu_cores: 8, load1: 1.23 })} />)
    expect(screen.queryByText(REGEX_CORES)).toBeNull()
    expect(screen.queryByText(REGEX_LOAD)).toBeNull()
  })
})

describe('MemoryCell', () => {
  it('renders used/total + swap pct', () => {
    render(
      <MemoryCell
        server={makeServer({
          mem_used: 7.2 * 1024 ** 3,
          mem_total: 16 * 1024 ** 3,
          swap_used: 0.1 * 1024 ** 3,
          swap_total: 4 * 1024 ** 3
        })}
      />
    )
    expect(screen.getByText(REGEX_MEM_USED_TOTAL)).toBeDefined()
    expect(screen.getByText(REGEX_SWAP)).toBeDefined()
    expect(screen.getByText(REGEX_3_PCT)).toBeDefined()
  })

  it('renders 0% swap when swap_total is 0', () => {
    render(<MemoryCell server={makeServer({ mem_used: 100, mem_total: 200, swap_used: 0, swap_total: 0 })} />)
    expect(screen.getByText(REGEX_SWAP_0_PCT)).toBeDefined()
  })

  it('hides sub-line when offline', () => {
    render(<MemoryCell server={makeServer({ online: false })} />)
    expect(screen.queryByText(REGEX_SWAP)).toBeNull()
  })
})

describe('DiskCell', () => {
  it('shows used/total text + r/w speeds when online', () => {
    render(
      <DiskCell
        server={makeServer({
          online: true,
          disk_used: 60_000_000_000,
          disk_total: 100_000_000_000,
          disk_read_bytes_per_sec: 2_100_000,
          disk_write_bytes_per_sec: 512_000
        })}
      />
    )
    expect(screen.getByText(REGEX_DISK_USED_TOTAL)).toBeDefined()
    expect(screen.getByText(REGEX_DISK_READ)).toBeDefined()
    expect(screen.getByText(REGEX_DISK_WRITE)).toBeDefined()
  })

  it('hides r/w sub when offline', () => {
    render(
      <DiskCell server={makeServer({ online: false, disk_read_bytes_per_sec: 999, disk_write_bytes_per_sec: 999 })} />
    )
    expect(screen.queryByText(REGEX_KB_PER_SEC)).toBeNull()
  })

  it('renders 0 B / 0 B when disk_total is 0', () => {
    render(<DiskCell server={makeServer({ disk_total: 0, disk_used: 0 })} />)
    expect(screen.getByText(REGEX_DISK_ZERO)).toBeDefined()
  })
})

const GB = 1024 ** 3
const TB = 1024 ** 4

function makeEntry(overrides: Partial<TrafficOverviewItem>): TrafficOverviewItem {
  return {
    billing_cycle: null,
    cycle_in: 0,
    cycle_out: 0,
    days_remaining: null,
    name: 'srv',
    percent_used: null,
    server_id: 'srv-1',
    traffic_limit: null,
    ...overrides
  }
}

describe('NetworkCell', () => {
  it('renders used/limit text + live ↓↑ when online', () => {
    render(
      <NetworkCell
        entry={makeEntry({ cycle_in: 50 * GB, cycle_out: 43.2 * GB, traffic_limit: 1 * TB })}
        server={makeServer({ online: true, net_in_speed: 1_153_434, net_out_speed: 339_968 })}
      />
    )
    expect(screen.getByText(REGEX_TRAFFIC_USED_LIMIT)).toBeDefined()
    expect(screen.getByText(REGEX_TRAFFIC_DOWN)).toBeDefined()
    expect(screen.getByText(REGEX_TRAFFIC_UP)).toBeDefined()
  })

  it('falls back to net_in_transfer + 1 TiB default when entry is undefined', () => {
    render(
      <NetworkCell
        entry={undefined}
        server={makeServer({ online: true, net_in_transfer: 2 * GB, net_out_transfer: 1 * GB })}
      />
    )
    expect(screen.getByText(REGEX_TRAFFIC_FALLBACK)).toBeDefined()
  })

  it('renders used/limit text and hides speeds when offline', () => {
    render(
      <NetworkCell
        entry={makeEntry({ cycle_in: 50 * GB, cycle_out: 50 * GB, traffic_limit: 1 * TB })}
        server={makeServer({ online: false })}
      />
    )
    expect(screen.getByText(REGEX_TRAFFIC_OFFLINE_USAGE)).toBeDefined()
    expect(screen.queryByText(REGEX_MB_PER_SEC)).toBeNull()
    expect(screen.queryByText(REGEX_KB_PER_SEC)).toBeNull()
  })

  it('treats traffic_limit <= 0 as fallback to default', () => {
    render(<NetworkCell entry={makeEntry({ traffic_limit: 0 })} server={makeServer({ online: true })} />)
    expect(screen.getByText(REGEX_LIMIT_DEFAULT)).toBeDefined()
  })
})

describe('UptimeCell', () => {
  const NOW = 1_700_000_000
  const _originalNow = Date.now
  beforeEach(() => {
    Date.now = () => NOW * 1000
  })
  afterEach(() => {
    Date.now = _originalNow
  })

  it('shows uptime + OS line when online', () => {
    render(
      <UptimeCell server={makeServer({ online: true, uptime: 23 * 86_400, os: 'Ubuntu 22.04', last_active: NOW })} />
    )
    expect(screen.getByText(REGEX_UPTIME_23D)).toBeDefined()
    expect(screen.getByText(REGEX_OS_UBUNTU)).toBeDefined()
  })

  it('shows offline + last-seen relative when offline', () => {
    render(
      <UptimeCell server={makeServer({ online: false, uptime: 0, os: 'Ubuntu 22.04', last_active: NOW - 7200 })} />
    )
    expect(screen.getByText(REGEX_OFFLINE)).toBeDefined()
    expect(screen.getByText(REGEX_LAST_SEEN)).toBeDefined()
  })
})

describe('NameCell', () => {
  it('renders single-line layout when no tags', () => {
    const { container } = render(<NameCell server={makeServer({ name: 'tokyo-1', tags: [] })} />)
    expect(screen.getByText('tokyo-1')).toBeDefined()
    expect(container.querySelector('[data-slot="tag-chip"]')).toBeNull()
  })

  it('renders chips under the name when tags present', () => {
    render(<NameCell server={makeServer({ name: 'tokyo-1', tags: ['prod', 'web'] })} />)
    expect(screen.getByText('prod')).toBeDefined()
    expect(screen.getByText('web')).toBeDefined()
  })
})

describe('NameCell rightSlot', () => {
  it('renders the rightSlot next to the server name', () => {
    render(<NameCell rightSlot={<span data-testid="slot" />} server={makeServer({ name: 'web-01' })} />)
    expect(screen.getByTestId('slot')).toBeDefined()
    expect(screen.getByText('web-01')).toBeDefined()
  })
})
