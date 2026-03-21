import { useTranslation } from 'react-i18next'
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from 'recharts'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  type ChartConfig,
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent
} from '@/components/ui/chart'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import type { DiskIoChartPoint, DiskIoSeries } from '@/lib/disk-io'
import { formatSpeed } from '@/lib/utils'

interface DiskIoChartProps {
  mergedData: DiskIoChartPoint[]
  perDiskData: DiskIoSeries[]
}

function DiskIoLineChart({ config, data }: { config: ChartConfig; data: DiskIoChartPoint[] }) {
  return (
    <ChartContainer className="h-[260px] w-full" config={config}>
      <LineChart accessibilityLayer data={data}>
        <CartesianGrid vertical={false} />
        <XAxis
          axisLine={false}
          dataKey="timestamp"
          tickFormatter={(value: string) =>
            new Date(value).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
          }
          tickLine={false}
        />
        <YAxis axisLine={false} tickFormatter={formatSpeed} tickLine={false} width={70} />
        <ChartTooltip content={<ChartTooltipContent hideLabel valueFormatter={(value) => formatSpeed(value)} />} />
        <ChartLegend content={<ChartLegendContent />} />
        <Line
          animationDuration={800}
          dataKey="read_bytes_per_sec"
          dot={false}
          stroke="var(--color-read_bytes_per_sec)"
          strokeWidth={2}
          type="monotone"
        />
        <Line
          animationDuration={800}
          dataKey="write_bytes_per_sec"
          dot={false}
          stroke="var(--color-write_bytes_per_sec)"
          strokeWidth={2}
          type="monotone"
        />
      </LineChart>
    </ChartContainer>
  )
}

export function DiskIoChart({ mergedData, perDiskData }: DiskIoChartProps) {
  const { t } = useTranslation('servers')

  if (mergedData.length === 0 && perDiskData.length === 0) {
    return null
  }

  const chartConfig = {
    read_bytes_per_sec: { label: t('disk_io_read'), color: 'var(--chart-1)' },
    write_bytes_per_sec: { label: t('disk_io_write'), color: 'var(--chart-2)' }
  } satisfies ChartConfig

  return (
    <Card className="mt-4">
      <Tabs defaultValue="merged">
        <CardHeader className="gap-3 sm:flex-row sm:items-center sm:justify-between">
          <CardTitle>{t('chart_disk_io')}</CardTitle>
          <TabsList>
            <TabsTrigger value="merged">{t('disk_io_merged')}</TabsTrigger>
            <TabsTrigger value="per-disk">{t('disk_io_per_disk')}</TabsTrigger>
          </TabsList>
        </CardHeader>

        <CardContent>
          <TabsContent value="merged">
            <DiskIoLineChart config={chartConfig} data={mergedData} />
          </TabsContent>

          <TabsContent value="per-disk">
            <div className="space-y-4">
              {perDiskData.map((series) => (
                <div key={series.name}>
                  <h4 className="mb-3 font-medium text-sm">{series.name}</h4>
                  <DiskIoLineChart config={chartConfig} data={series.data} />
                </div>
              ))}
            </div>
          </TabsContent>
        </CardContent>
      </Tabs>
    </Card>
  )
}
