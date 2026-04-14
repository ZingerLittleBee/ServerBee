import type { Column, ColumnDef, Table as TanstackTable } from '@tanstack/react-table'
import { flexRender } from '@tanstack/react-table'
import { ArrowDown, ArrowUp, ArrowUpDown, ChevronLeft, ChevronRight } from 'lucide-react'
import { Checkbox } from '@/components/ui/checkbox'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { cn } from '@/lib/utils'
import { Button } from './button'

// ---------------------------------------------------------------------------
// DataTable – renders a TanStack Table instance with shadcn Table components
// ---------------------------------------------------------------------------

interface DataTableProps<TData> {
  className?: string
  noResults?: string
  table: TanstackTable<TData>
}

function DataTable<TData>({ table, noResults, className }: DataTableProps<TData>) {
  return (
    <div className={cn('overflow-hidden rounded-lg border', className)}>
      <Table className="table-fixed">
        <TableHeader className="sticky top-0 z-10 bg-background">
          {table.getHeaderGroups().map((headerGroup) => (
            <TableRow className="bg-muted/50 hover:bg-muted/50" key={headerGroup.id}>
              {headerGroup.headers.map((header) => (
                <TableHead className={header.column.columnDef.meta?.className} key={header.id}>
                  {header.isPlaceholder ? null : flexRender(header.column.columnDef.header, header.getContext())}
                </TableHead>
              ))}
            </TableRow>
          ))}
        </TableHeader>
        <TableBody>
          {table.getRowModel().rows.length > 0 ? (
            table.getRowModel().rows.map((row) => (
              <TableRow data-state={row.getIsSelected() && 'selected'} key={row.id}>
                {row.getVisibleCells().map((cell) => (
                  <TableCell className={cell.column.columnDef.meta?.className} key={cell.id}>
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </TableCell>
                ))}
              </TableRow>
            ))
          ) : (
            <TableRow>
              <TableCell className="h-24 text-center" colSpan={table.getAllColumns().length}>
                {noResults ?? 'No results.'}
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </Table>
    </div>
  )
}

// ---------------------------------------------------------------------------
// DataTableColumnHeader – sortable column header button
// ---------------------------------------------------------------------------

interface DataTableColumnHeaderProps<TData, TValue> {
  className?: string
  column: Column<TData, TValue>
  title: string
}

function DataTableColumnHeader<TData, TValue>({ column, title, className }: DataTableColumnHeaderProps<TData, TValue>) {
  if (!column.getCanSort()) {
    return <span className={cn('text-muted-foreground text-xs', className)}>{title}</span>
  }

  const sorted = column.getIsSorted()

  return (
    <button
      className={cn(
        'flex items-center gap-1 text-xs',
        sorted ? 'text-foreground' : 'text-muted-foreground hover:text-foreground',
        className
      )}
      onClick={column.getToggleSortingHandler()}
      type="button"
    >
      {title}
      {sorted === 'asc' && <ArrowUp className="size-3.5" />}
      {sorted === 'desc' && <ArrowDown className="size-3.5" />}
      {sorted === false && <ArrowUpDown className="size-3.5 opacity-40" />}
    </button>
  )
}

// ---------------------------------------------------------------------------
// DataTablePagination – prev/next pagination controls
// ---------------------------------------------------------------------------

interface DataTablePaginationProps<TData> {
  table: TanstackTable<TData>
}

function DataTablePagination<TData>({ table }: DataTablePaginationProps<TData>) {
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

// ---------------------------------------------------------------------------
// Selection column helper – creates a select column with header + cell checkbox
// ---------------------------------------------------------------------------

function createSelectColumn<TData>(): ColumnDef<TData> {
  return {
    id: 'select',
    header: ({ table }) => (
      <Checkbox
        checked={table.getIsAllPageRowsSelected()}
        onCheckedChange={(checked) => table.toggleAllPageRowsSelected(!!checked)}
      />
    ),
    cell: ({ row }) => (
      <Checkbox checked={row.getIsSelected()} onCheckedChange={(checked) => row.toggleSelected(!!checked)} />
    ),
    enableSorting: false,
    meta: { className: 'w-10' }
  }
}

export { DataTable, DataTableColumnHeader, DataTablePagination, createSelectColumn }

// ---------------------------------------------------------------------------
// Type augmentation – allow column meta to carry className
// ---------------------------------------------------------------------------
declare module '@tanstack/react-table' {
  interface ColumnMeta<TData, TValue> {
    className?: string
  }
}
