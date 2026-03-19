import { fireEvent, render, screen } from '@testing-library/react'
import { createContext, type ReactNode, useContext, useState } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { DiskIoChart } from './disk-io-chart'

const TabsContext = createContext<{ setValue: (value: string) => void; value: string } | null>(null)

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) =>
      ({
        chart_disk_io: 'Disk I/O',
        disk_io_merged: 'Merged',
        disk_io_per_disk: 'Per Disk',
        disk_io_read: 'Read',
        disk_io_write: 'Write'
      })[key] ?? key
  })
}))

vi.mock('@/components/ui/tabs', () => ({
  Tabs: ({ children, defaultValue }: { children?: ReactNode; defaultValue?: string }) => {
    const [value, setValue] = useState(defaultValue ?? '')
    return <TabsContext.Provider value={{ setValue, value }}>{children}</TabsContext.Provider>
  },
  TabsList: ({ children }: { children?: ReactNode }) => <div data-testid="tabs-list">{children}</div>,
  TabsTrigger: ({ children, value }: { children?: ReactNode; value: string }) => {
    const context = useContext(TabsContext)
    if (!context) {
      return null
    }

    return (
      <button onClick={() => context.setValue(value)} type="button">
        {children}
      </button>
    )
  },
  TabsContent: ({ children, value }: { children?: ReactNode; value: string }) => {
    const context = useContext(TabsContext)
    if (!context || context.value !== value) {
      return null
    }

    return <div data-testid={`tab-content-${value}`}>{children}</div>
  }
}))

vi.mock('recharts', () => {
  const createWrapper =
    (testId: string) =>
    ({ children }: { children?: ReactNode }) => <div data-testid={testId}>{children}</div>

  return {
    ResponsiveContainer: createWrapper('responsive-container'),
    Tooltip: createWrapper('chart-tooltip'),
    Legend: createWrapper('chart-legend'),
    CartesianGrid: () => <div data-testid="cartesian-grid" />,
    XAxis: () => <div data-testid="x-axis" />,
    YAxis: () => <div data-testid="y-axis" />,
    LineChart: ({ children }: { children?: ReactNode }) => <div data-testid="line-chart">{children}</div>,
    Line: ({ dataKey }: { dataKey: string }) => <div data-testid={`line-${dataKey}`} />
  }
})

describe('DiskIoChart', () => {
  it('renders merged and per-disk views', () => {
    render(
      <DiskIoChart
        mergedData={[{ timestamp: '2026-03-19T10:00:00Z', read_bytes_per_sec: 100, write_bytes_per_sec: 200 }]}
        perDiskData={[
          {
            name: 'sda',
            data: [{ timestamp: '2026-03-19T10:00:00Z', read_bytes_per_sec: 100, write_bytes_per_sec: 200 }]
          },
          {
            name: 'sdb',
            data: [{ timestamp: '2026-03-19T10:00:00Z', read_bytes_per_sec: 50, write_bytes_per_sec: 100 }]
          }
        ]}
      />
    )

    expect(screen.getByText('Disk I/O')).toBeInTheDocument()
    expect(screen.getByTestId('tab-content-merged')).toBeInTheDocument()
    expect(screen.getByTestId('line-read_bytes_per_sec')).toBeInTheDocument()
    expect(screen.getByTestId('line-write_bytes_per_sec')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Per Disk' }))

    expect(screen.getByTestId('tab-content-per-disk')).toBeInTheDocument()
    expect(screen.getByText('sda')).toBeInTheDocument()
    expect(screen.getByText('sdb')).toBeInTheDocument()
  })

  it('returns null when there is no disk I/O data', () => {
    const { container } = render(<DiskIoChart mergedData={[]} perDiskData={[]} />)

    expect(container).toBeEmptyDOMElement()
  })
})
