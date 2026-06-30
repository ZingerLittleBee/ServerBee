import { type ColumnDef, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { DataTable } from './data-table'
import { DataTableViewOptions } from './data-table-view-options'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key
  })
}))

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

function TestViewOptions() {
  const table = useReactTable({
    data: [{ name: 'server-1' }],
    columns,
    getCoreRowModel: getCoreRowModel()
  })

  return <DataTableViewOptions table={table} />
}

describe('DataTableViewOptions', () => {
  it('exposes the combobox popup relationship to assistive technology', () => {
    render(<TestViewOptions />)

    const trigger = screen.getByRole('combobox', { name: 'table.toggle_columns' })

    expect(trigger).toHaveAttribute('aria-expanded', 'false')
    expect(trigger).toHaveAttribute('aria-controls')
  })
})
