import { flexRender, type Row, type Table as TanstackTable } from '@tanstack/react-table'
import type * as React from 'react'
import { useTranslation } from 'react-i18next'
import { DataTablePagination } from '@/components/data-table/data-table-pagination'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { getColumnPinningStyle } from '@/lib/data-table'
import { cn } from '@/lib/utils'

interface DataTableProps<TData> extends React.ComponentProps<'div'> {
  actionBar?: React.ReactNode
  /** Fill remaining vertical space with a sticky header and internally scrollable body. */
  fillHeight?: boolean
  /** Hide the pagination/action-bar footer entirely. Use when the table renders
   *  all rows (e.g. embedded in a content-sized dashboard widget). */
  hidePagination?: boolean
  /** Optional per-row className, e.g. to dim disabled/offline rows. */
  rowClassName?: (row: Row<TData>) => string | false | undefined
  table: TanstackTable<TData>
}

export function DataTable<TData>({
  table,
  actionBar,
  children,
  className,
  fillHeight = false,
  hidePagination = false,
  rowClassName,
  ...props
}: DataTableProps<TData>) {
  const { t } = useTranslation('common')
  return (
    <div
      className={cn('flex w-full min-w-0 flex-col gap-2.5 overflow-hidden', fillHeight && 'min-h-0 flex-1', className)}
      {...props}
    >
      {children}
      <div
        className={cn(
          'min-w-0 max-w-full rounded-md border',
          fillHeight ? 'flex min-h-0 flex-1 flex-col overflow-hidden' : 'overflow-x-auto'
        )}
        data-testid="data-table-scroll"
      >
        <Table className="min-w-full table-fixed [&_td:last-child]:pr-5 [&_td]:px-3 [&_th:last-child]:pr-5 [&_th]:px-3">
          <TableHeader className={cn(fillHeight && 'sticky top-0 z-10 bg-background')}>
            {table.getHeaderGroups().map((headerGroup) => (
              <TableRow key={headerGroup.id}>
                {headerGroup.headers.map((header) => (
                  <TableHead
                    className={header.column.columnDef.meta?.className}
                    colSpan={header.colSpan}
                    key={header.id}
                    style={{
                      ...getColumnPinningStyle({ column: header.column })
                    }}
                  >
                    {header.isPlaceholder ? null : flexRender(header.column.columnDef.header, header.getContext())}
                  </TableHead>
                ))}
              </TableRow>
            ))}
          </TableHeader>
          <TableBody>
            {table.getRowModel().rows?.length ? (
              table.getRowModel().rows.map((row) => (
                <TableRow
                  className={cn(rowClassName?.(row))}
                  data-state={row.getIsSelected() && 'selected'}
                  key={row.id}
                >
                  {row.getVisibleCells().map((cell) => (
                    <TableCell
                      className={cn(cell.column.columnDef.meta?.className, cell.column.columnDef.meta?.cellClassName)}
                      key={cell.id}
                      style={{
                        ...getColumnPinningStyle({ column: cell.column })
                      }}
                    >
                      {flexRender(cell.column.columnDef.cell, cell.getContext())}
                    </TableCell>
                  ))}
                </TableRow>
              ))
            ) : (
              <TableRow>
                <TableCell className="h-24 text-center" colSpan={table.getAllColumns().length}>
                  {t('table.no_results')}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>
      {!hidePagination && (
        <div className="flex flex-col gap-2.5">
          <DataTablePagination table={table} />
          {actionBar && table.getFilteredSelectedRowModel().rows.length > 0 && actionBar}
        </div>
      )}
    </div>
  )
}
