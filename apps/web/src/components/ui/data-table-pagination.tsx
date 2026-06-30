import type { Table as TanstackTable } from '@tanstack/react-table'
import { ChevronLeft, ChevronRight } from 'lucide-react'
import { Button } from './button'

interface DataTablePaginationProps<TData> {
  table: TanstackTable<TData>
}

export function DataTablePagination<TData>({ table }: DataTablePaginationProps<TData>) {
  return (
    <div className="mt-3 flex items-center justify-between text-muted-foreground text-sm">
      <span>
        Page {table.getState().pagination.pageIndex + 1} of {table.getPageCount()}
      </span>
      <div className="flex gap-1">
        <Button disabled={!table.getCanPreviousPage()} onClick={() => table.previousPage()} size="sm" variant="outline">
          <ChevronLeft className="size-4" />
        </Button>
        <Button disabled={!table.getCanNextPage()} onClick={() => table.nextPage()} size="sm" variant="outline">
          <ChevronRight className="size-4" />
        </Button>
      </div>
    </div>
  )
}
