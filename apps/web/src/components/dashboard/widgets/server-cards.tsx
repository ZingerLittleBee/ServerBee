import { getCoreRowModel, getSortedRowModel, type SortingState, useReactTable } from '@tanstack/react-table'
import { Loader2 } from 'lucide-react'
import { type RefObject, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { DataTable } from '@/components/data-table/data-table'
import { ServerCard } from '@/components/server/server-card'
import { useCostOverview } from '@/hooks/use-cost'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { useTrafficOverview } from '@/hooks/use-traffic-overview'
import { filterByIds } from '@/lib/widget-helpers'
import type { ServerCardsConfig } from '@/lib/widget-types'
import { buildServerColumns } from '@/routes/_authed/servers/components/server-columns'

interface ServerCardsWidgetProps {
  config: ServerCardsConfig
  servers: ServerMetrics[]
}

// Soft cap on the first render. The widget grows to fit its content (no inner
// scroll), so rendering hundreds of cards/rows up front would jank. Instead we
// reveal the first batch and load the next as the user scrolls the dashboard
// near the widget's bottom (page-scroll driven, compatible with content-height).
const REVEAL_STEP = 50

// Reveals `step` items at a time, loading the next batch when the sentinel
// scrolls into view (page-scroll driven, no inner scroll container needed).
function useIncrementalReveal(total: number, step = REVEAL_STEP) {
  const [count, setCount] = useState(step)
  const sentinelRef = useRef<HTMLDivElement>(null)
  const visibleCount = Math.min(count, total)
  const hasMore = visibleCount < total

  // Re-subscribe on every batch: re-observing re-fires the initial notification
  // if the sentinel is still on-screen (IntersectionObserver won't re-notify a
  // sentinel that stays intersecting), so short lists keep filling until the
  // sentinel is pushed off-screen or everything is revealed.
  // biome-ignore lint/correctness/useExhaustiveDependencies: visibleCount is an intentional re-subscribe trigger, not used in the body
  useEffect(() => {
    const el = sentinelRef.current
    if (!hasMore || el === null || typeof IntersectionObserver === 'undefined') {
      return
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setCount((c) => c + step)
        }
      },
      { rootMargin: '300px' }
    )
    observer.observe(el)
    return () => observer.disconnect()
  }, [hasMore, step, visibleCount])

  return { visibleCount, hasMore, sentinelRef }
}

// Columns the list layout hides relative to the full servers page: selection and
// per-row edit actions are page-only interactions, and group/status-dot are
// hidden on the servers page by default too. The remaining data columns render
// identically to /servers?view=table.
const HIDDEN_LIST_COLUMNS = { select: false, 'status-dot': false, group: false, actions: false }

// Reuses the exact servers-page table (DataTable + shared columns) so the list
// layout is pixel-identical to /servers?view=table. State is kept local (no URL
// sync) and the pagination footer is hidden — the widget reveals rows on scroll.
function ServerListTable({ servers }: { servers: ServerMetrics[] }) {
  const { t } = useTranslation(['servers'])
  const [sorting, setSorting] = useState<SortingState>([{ id: 'name', desc: false }])
  const { data: trafficOverview = [] } = useTrafficOverview()
  const { data: costOverview } = useCostOverview()

  const costByServerId = useMemo(() => {
    const entries = costOverview?.servers ?? []
    return new Map(entries.map((entry) => [entry.server_id, entry]))
  }, [costOverview])

  const columns = useMemo(
    () =>
      buildServerColumns({
        t,
        costByServerId,
        groupMap: new Map(),
        groupOptions: [],
        statusOptions: [],
        selectMode: false,
        onEdit: () => undefined,
        trafficOverview
      }),
    [t, costByServerId, trafficOverview]
  )

  const table = useReactTable({
    data: servers,
    columns,
    state: { sorting, columnVisibility: HIDDEN_LIST_COLUMNS },
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getRowId: (row) => row.id
  })

  return (
    <DataTable hidePagination rowClassName={(row) => !row.original.online && 'opacity-45 grayscale'} table={table} />
  )
}

// The sentinel doubles as the load-more indicator: it sits at the bottom of the
// table/grid, triggers the next batch when scrolled into view, and shows a
// spinner while more rows remain. It unmounts once everything is revealed.
function LoadMoreSentinel({ sentinelRef }: { sentinelRef: RefObject<HTMLDivElement | null> }) {
  const { t } = useTranslation('common')
  return (
    <div className="flex items-center justify-center gap-2 py-4 text-muted-foreground text-sm" ref={sentinelRef}>
      <Loader2 aria-hidden="true" className="size-4 animate-spin" />
      {t('loading')}
    </div>
  )
}

export function ServerCardsWidget({ config, servers }: ServerCardsWidgetProps) {
  const filtered = useMemo(() => filterByIds(servers, config.server_ids, (s) => s.id), [servers, config.server_ids])
  const { visibleCount, hasMore, sentinelRef } = useIncrementalReveal(filtered.length)
  const visible = useMemo(() => filtered.slice(0, visibleCount), [filtered, visibleCount])

  if (filtered.length === 0) {
    return (
      // data-measure: empty-state height is measured the same as the populated
      // layouts so the grid cell shrinks to fit instead of leaving dead space.
      <div className="flex items-center justify-center py-8 text-muted-foreground text-sm" data-measure>
        No servers to display
      </div>
    )
  }

  if (config.layout === 'list') {
    return (
      // data-measure: natural content height (grows with the revealed rows),
      // measured by the grid to size the cell — never height-capped or scrolled.
      <div data-measure>
        <ServerListTable servers={visible} />
        {hasMore && <LoadMoreSentinel sentinelRef={sentinelRef} />}
      </div>
    )
  }

  return (
    // data-measure: natural content height (grows with the revealed cards),
    // measured by the grid to size the cell so the widget is never height-capped
    // or scrolled — it always hugs exactly as many rows of cards as it renders.
    <div data-measure>
      <div
        className="grid content-start gap-4"
        style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))' }}
      >
        {visible.map((server) => (
          // content-visibility:auto skips layout/paint for off-screen cards;
          // contain-intrinsic-size reserves their height so scrolling stays smooth.
          <div className="[contain-intrinsic-size:auto_280px] [content-visibility:auto]" key={server.id}>
            <ServerCard server={server} />
          </div>
        ))}
      </div>
      {hasMore && <LoadMoreSentinel sentinelRef={sentinelRef} />}
    </div>
  )
}
