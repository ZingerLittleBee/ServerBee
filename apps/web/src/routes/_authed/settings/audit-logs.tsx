import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { ChevronLeft, ChevronRight } from 'lucide-react'
import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api-client'

export const Route = createFileRoute('/_authed/settings/audit-logs')({
  component: AuditLogsPage
})

interface AuditLogEntry {
  action: string
  created_at: string
  detail: string | null
  id: number
  ip: string
  user_id: string
}

interface AuditListResponse {
  entries: AuditLogEntry[]
  total: number
}

const PAGE_SIZE = 25

function AuditLogsPage() {
  const [page, setPage] = useState(0)

  const { data, isLoading } = useQuery<AuditListResponse>({
    queryKey: ['audit-logs', page],
    queryFn: () => api.get<AuditListResponse>(`/api/audit-logs?limit=${PAGE_SIZE}&offset=${page * PAGE_SIZE}`)
  })

  const total = data?.total ?? 0
  const entries = data?.entries ?? []
  const totalPages = Math.ceil(total / PAGE_SIZE)

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">Audit Logs</h1>

      <div className="max-w-4xl">
        <div className="rounded-lg border bg-card">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b">
                  <th className="px-4 py-3 text-left font-medium text-muted-foreground">Time</th>
                  <th className="px-4 py-3 text-left font-medium text-muted-foreground">Action</th>
                  <th className="px-4 py-3 text-left font-medium text-muted-foreground">User</th>
                  <th className="px-4 py-3 text-left font-medium text-muted-foreground">IP</th>
                  <th className="px-4 py-3 text-left font-medium text-muted-foreground">Detail</th>
                </tr>
              </thead>
              <tbody>
                {isLoading &&
                  Array.from({ length: 5 }, (_, i) => (
                    <tr className="border-b" key={`skeleton-${i.toString()}`}>
                      <td className="px-4 py-3" colSpan={5}>
                        <div className="h-5 animate-pulse rounded bg-muted" />
                      </td>
                    </tr>
                  ))}
                {!isLoading && entries.length === 0 && (
                  <tr>
                    <td className="px-4 py-8 text-center text-muted-foreground" colSpan={5}>
                      No audit log entries
                    </td>
                  </tr>
                )}
                {entries.map((entry) => (
                  <tr className="border-b last:border-0" key={entry.id}>
                    <td className="whitespace-nowrap px-4 py-3 text-muted-foreground">
                      {new Date(entry.created_at).toLocaleString()}
                    </td>
                    <td className="px-4 py-3">
                      <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs">{entry.action}</span>
                    </td>
                    <td className="px-4 py-3 font-mono text-muted-foreground text-xs">{entry.user_id.slice(0, 8)}</td>
                    <td className="px-4 py-3 text-muted-foreground">{entry.ip}</td>
                    <td className="max-w-xs truncate px-4 py-3 text-muted-foreground">{entry.detail || '-'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {totalPages > 1 && (
            <div className="flex items-center justify-between border-t px-4 py-3">
              <span className="text-muted-foreground text-sm">
                {total} entries, page {page + 1} of {totalPages}
              </span>
              <div className="flex gap-1">
                <Button disabled={page === 0} onClick={() => setPage((p) => p - 1)} size="sm" variant="outline">
                  <ChevronLeft className="size-4" />
                </Button>
                <Button
                  disabled={page >= totalPages - 1}
                  onClick={() => setPage((p) => p + 1)}
                  size="sm"
                  variant="outline"
                >
                  <ChevronRight className="size-4" />
                </Button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
