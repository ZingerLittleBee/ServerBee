import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { type ColumnDef, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import { Trash2 } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { DataTable, DataTablePagination } from '@/components/ui/data-table'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { AuditListResponse, AuditLogEntry, AuditOptionsResponse } from '@/lib/api-schema'

interface AuditClearResponse {
  deleted: number
}

const ALL_VALUE = '__all__'

interface AuditSearch {
  action: string
  page: number
  user_id: string
}

export const Route = createFileRoute('/_authed/settings/audit-logs')({
  validateSearch: (search: Record<string, unknown>): AuditSearch => ({
    page: Number(search.page) || 0,
    action: typeof search.action === 'string' ? search.action : '',
    user_id: typeof search.user_id === 'string' ? search.user_id : ''
  }),
  component: AuditLogsPage
})

const PAGE_SIZE = 25

function AuditLogsPage() {
  const { t } = useTranslation('settings')
  const queryClient = useQueryClient()
  const { page, action, user_id } = Route.useSearch()
  const navigate = Route.useNavigate()
  const [clearOpen, setClearOpen] = useState(false)

  const { data: options } = useQuery<AuditOptionsResponse>({
    queryKey: ['audit-logs', 'options'],
    queryFn: () => api.get<AuditOptionsResponse>('/api/audit-logs/options'),
    staleTime: 60_000
  })

  const userLabelMap = useMemo(() => {
    const map = new Map<string, string>()
    for (const u of options?.users ?? []) {
      map.set(u.id, u.label)
    }
    return map
  }, [options])

  const actionItems = useMemo(
    () => [
      { value: ALL_VALUE, label: t('audit.filter_all') },
      ...(options?.actions ?? []).map((a) => ({ value: a, label: a }))
    ],
    [options, t]
  )

  const userItems = useMemo(
    () => [
      { value: ALL_VALUE, label: t('audit.filter_all') },
      ...(options?.users ?? []).map((u) => ({ value: u.id, label: u.label }))
    ],
    [options, t]
  )

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
        cell: ({ getValue }) => {
          const id = getValue<string>()
          const label = userLabelMap.get(id) ?? id
          return <span className="text-muted-foreground text-sm">{label}</span>
        },
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
        enableSorting: false
      }
    ],
    [t, userLabelMap]
  )

  const { data, isLoading } = useQuery<AuditListResponse>({
    queryKey: ['audit-logs', page, action, user_id],
    queryFn: () => {
      const params = new URLSearchParams({
        limit: String(PAGE_SIZE),
        offset: String(page * PAGE_SIZE)
      })
      if (action) {
        params.set('action', action)
      }
      if (user_id) {
        params.set('user_id', user_id)
      }
      return api.get<AuditListResponse>(`/api/audit-logs?${params.toString()}`)
    },
    placeholderData: (prev) => prev
  })

  const clearMutation = useMutation({
    mutationFn: () => api.delete<AuditClearResponse>('/api/audit-logs'),
    onSuccess: (res) => {
      toast.success(t('audit.clear_success', { count: res.deleted }))
      queryClient.invalidateQueries({ queryKey: ['audit-logs'] })
      setClearOpen(false)
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('audit.clear_failed'))
    }
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
      navigate({ search: (prev) => ({ ...prev, page: newState.pageIndex }) })
    }
  })

  const handleActionChange = (value: string | null) => {
    const next = value && value !== ALL_VALUE ? value : ''
    navigate({ search: (prev) => ({ ...prev, action: next, page: 0 }) })
  }

  const handleUserChange = (value: string | null) => {
    const next = value && value !== ALL_VALUE ? value : ''
    navigate({ search: (prev) => ({ ...prev, user_id: next, page: 0 }) })
  }

  return (
    <div className="w-full min-w-0 max-w-[calc(100vw-1.5rem)] overflow-hidden sm:max-w-full">
      <div className="mb-6 flex items-center justify-end gap-4">
        <AlertDialog onOpenChange={setClearOpen} open={clearOpen}>
          <AlertDialogTrigger
            render={
              <Button disabled={clearMutation.isPending || total === 0} size="sm" variant="destructive">
                <Trash2 aria-hidden="true" className="mr-1.5 size-4" />
                {t('audit.clear')}
              </Button>
            }
          />
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t('audit.clear_confirm_title')}</AlertDialogTitle>
              <AlertDialogDescription>{t('audit.clear_confirm_description')}</AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
              <AlertDialogAction
                disabled={clearMutation.isPending}
                onClick={() => clearMutation.mutate()}
                variant="destructive"
              >
                {t('audit.clear')}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>

      <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center sm:gap-4">
        <div className="flex w-full flex-col gap-1 sm:w-56">
          <label className="text-muted-foreground text-xs" htmlFor="audit-filter-action">
            {t('audit.filter_action')}
          </label>
          <Select items={actionItems} onValueChange={handleActionChange} value={action || ALL_VALUE}>
            <SelectTrigger className="w-full" id="audit-filter-action">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {actionItems.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {item.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="flex w-full flex-col gap-1 sm:w-56">
          <label className="text-muted-foreground text-xs" htmlFor="audit-filter-user">
            {t('audit.filter_user')}
          </label>
          <Select items={userItems} onValueChange={handleUserChange} value={user_id || ALL_VALUE}>
            <SelectTrigger className="w-full" id="audit-filter-user">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {userItems.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {item.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

      <div className="w-full min-w-0">
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
