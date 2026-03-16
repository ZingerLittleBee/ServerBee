import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { type ColumnDef, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { DataTable, DataTablePagination } from '@/components/ui/data-table'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { AuditListResponse, AuditLogEntry } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/audit-logs')({
  validateSearch: (search: Record<string, unknown>) => ({
    page: Number(search.page) || 0
  }),
  component: AuditLogsPage
})

const PAGE_SIZE = 25

function AuditLogsPage() {
  const { t } = useTranslation('settings')
  const { page } = Route.useSearch()
  const navigate = Route.useNavigate()

  const columns = useMemo<ColumnDef<AuditLogEntry>[]>(
    () => [
      {
        accessorKey: 'created_at',
        header: t('audit.col_time'),
        cell: ({ getValue }) => (
          <span className="whitespace-nowrap text-muted-foreground">
            {new Date(getValue<string>()).toLocaleString()}
          </span>
        ),
        enableSorting: false
      },
      {
        accessorKey: 'action',
        header: t('audit.col_action'),
        cell: ({ getValue }) => (
          <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs">{getValue<string>()}</span>
        ),
        enableSorting: false
      },
      {
        accessorKey: 'user_id',
        header: t('audit.col_user'),
        cell: ({ getValue }) => (
          <span className="font-mono text-muted-foreground text-xs">{getValue<string>().slice(0, 8)}</span>
        ),
        enableSorting: false
      },
      {
        accessorKey: 'ip',
        header: t('audit.col_ip'),
        cell: ({ getValue }) => <span className="text-muted-foreground">{getValue<string>()}</span>,
        enableSorting: false
      },
      {
        accessorKey: 'detail',
        header: t('audit.col_detail'),
        cell: ({ getValue }) => (
          <span className="block truncate text-muted-foreground">{getValue<string | null>() || '-'}</span>
        ),
        enableSorting: false,
        meta: { className: 'max-w-xs' }
      }
    ],
    [t]
  )

  const { data, isLoading } = useQuery<AuditListResponse>({
    queryKey: ['audit-logs', page],
    queryFn: () => api.get<AuditListResponse>(`/api/audit-logs?limit=${PAGE_SIZE}&offset=${page * PAGE_SIZE}`),
    placeholderData: (prev) => prev
  })

  const total = data?.total ?? 0
  const entries = data?.entries ?? []
  const totalPages = Math.ceil(total / PAGE_SIZE)

  const table = useReactTable({
    data: entries,
    columns,
    getCoreRowModel: getCoreRowModel(),
    manualPagination: true,
    pageCount: totalPages,
    state: {
      pagination: { pageIndex: page, pageSize: PAGE_SIZE }
    },
    onPaginationChange: (updater) => {
      const newState = typeof updater === 'function' ? updater({ pageIndex: page, pageSize: PAGE_SIZE }) : updater
      navigate({ search: { page: newState.pageIndex } })
    }
  })

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">{t('audit.title')}</h1>

      <div className="max-w-4xl">
        {isLoading && !data ? (
          <div className="space-y-2">
            {Array.from({ length: 5 }, (_, i) => (
              <Skeleton className="h-10 w-full" key={`skeleton-${i.toString()}`} />
            ))}
          </div>
        ) : (
          <>
            <DataTable noResults={t('audit.no_entries')} table={table} />
            {totalPages > 1 && (
              <>
                <DataTablePagination table={table} />
                <p className="mt-1 text-center text-muted-foreground text-xs">
                  {t('audit.pagination', { total, page: page + 1, pages: totalPages })}
                </p>
              </>
            )}
          </>
        )}
      </div>
    </div>
  )
}
