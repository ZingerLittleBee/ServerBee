import { type ColumnDef, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { DataTable } from './data-table'

interface Row {
  name: string
}

const columns: ColumnDef<Row>[] = [
  {
    accessorKey: 'name',
    header: 'Name',
    cell: ({ row }) => row.original.name
  }
]

function TestTable() {
  const table = useReactTable({
    data: [{ name: 'server-1' }],
    columns,
    getCoreRowModel: getCoreRowModel()
  })

  return <DataTable table={table} />
}

describe('DataTable mobile layout', () => {
  it('keeps horizontal overflow inside the table viewport', () => {
    const { container } = render(<TestTable />)

    expect(container.firstElementChild).toHaveClass('min-w-0', 'overflow-hidden')
    expect(screen.getByTestId('data-table-scroll')).toHaveClass('min-w-0')
    expect(screen.getByRole('table')).toHaveClass('min-w-full')
  })
})
