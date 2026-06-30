import type { Table as TanstackTable } from '@tanstack/react-table'
import { flexRender } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { cn } from '@/lib/utils'

// ---------------------------------------------------------------------------
// DataTable – renders a TanStack Table instance with shadcn Table components
// ---------------------------------------------------------------------------

interface DataTableProps<TData> {
  className?: string
  noResults?: string
  table: TanstackTable<TData>
}

function DataTable<TData>({ table, noResults, className }: DataTableProps<TData>) {
  const { t } = useTranslation('common')
  return (
    <div className={cn('min-w-0 max-w-full overflow-hidden rounded-lg border', className)}>
      <Table>
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
                {noResults ?? t('table.no_results')}
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </Table>
    </div>
  )
}

export { DataTable }

// ---------------------------------------------------------------------------
// Type augmentation – allow column meta to carry className
// ---------------------------------------------------------------------------
declare module '@tanstack/react-table' {
  interface ColumnMeta<TData, TValue> {
    className?: string
  }
}
