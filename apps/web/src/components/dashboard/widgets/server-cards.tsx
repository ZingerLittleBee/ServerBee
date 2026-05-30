import { getCoreRowModel, getSortedRowModel, type SortingState, useReactTable } from '@tanstack/react-table'
import { useMemo, useState } from 'react'
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

// Columns the list layout hides relative to the full servers page: selection and
// per-row edit actions are page-only interactions, and group/status-dot are
// hidden on the servers page by default too. The remaining data columns render
// identically to /servers?view=table.
const HIDDEN_LIST_COLUMNS = { select: false, 'status-dot': false, group: false, actions: false }

// Reuses the exact servers-page table (DataTable + shared columns) so the list
// layout is pixel-identical to /servers?view=table. State is kept local (no URL
// sync) and pagination is omitted — the widget grows to fit all rows.
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

  return <DataTable rowClassName={(row) => !row.original.online && 'opacity-45 grayscale'} table={table} />
}

export function ServerCardsWidget({ config, servers }: ServerCardsWidgetProps) {
  const filtered = useMemo(() => filterByIds(servers, config.server_ids, (s) => s.id), [servers, config.server_ids])

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
      // data-measure: natural content height (grows with the number of rows),
      // measured by the grid to size the cell — never height-capped or scrolled.
      <div data-measure>
        <ServerListTable servers={filtered} />
      </div>
    )
  }

  return (
    // data-measure: natural content height (grows with the number of cards),
    // measured by the grid to size the cell so the widget is never height-capped
    // or scrolled — it always hugs exactly as many rows of cards as it renders.
    <div
      className="grid content-start gap-4"
      data-measure
      style={{
        gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))'
      }}
    >
      {filtered.map((server) => (
        <ServerCard key={server.id} server={server} />
      ))}
    </div>
  )
}
