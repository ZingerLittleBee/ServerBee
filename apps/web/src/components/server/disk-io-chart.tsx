import { useTranslation } from 'react-i18next'
import { MetricLinePlot, type MetricLineSeries } from '@/components/charts/metric-line-plot'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import type { DiskIoChartPoint, DiskIoSeries } from '@/lib/disk-io'
import { formatSpeed } from '@/lib/utils'

function defaultFormatTime(time: string): string {
  const d = new Date(time)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

interface DiskIoChartProps {
  formatTime?: (time: string) => string
  mergedData: DiskIoChartPoint[]
  perDiskData: DiskIoSeries[]
}

function DiskIoLineChart({
  ariaLabel,
  data,
  formatTime = defaultFormatTime,
  series,
  timeLabel
}: {
  ariaLabel: string
  data: DiskIoChartPoint[]
  formatTime?: (time: string) => string
  series: MetricLineSeries[]
  timeLabel: string
}) {
  return (
    <MetricLinePlot
      ariaLabel={ariaLabel}
      className="h-[260px] w-full"
      data={data}
      formatTime={formatTime}
      formatTooltipLabel={formatTime}
      formatValue={formatSpeed}
      formatYAxisValue={formatSpeed}
      series={series}
      timeLabel={timeLabel}
    />
  )
}

export function DiskIoChart({ formatTime, mergedData, perDiskData }: DiskIoChartProps) {
  const { t } = useTranslation('servers')

  if (mergedData.length === 0 && perDiskData.length === 0) {
    return null
  }

  const chartSeries: MetricLineSeries[] = [
    { dataKey: 'read_bytes_per_sec', label: t('disk_io_read'), color: 'var(--chart-1)' },
    { dataKey: 'write_bytes_per_sec', label: t('disk_io_write'), color: 'var(--chart-2)' }
  ]

  return (
    <Card className="min-w-0">
      <Tabs defaultValue="merged">
        <CardHeader className="gap-3 sm:flex-row sm:items-center sm:justify-between">
          <CardTitle>{t('chart_disk_io')}</CardTitle>
          <TabsList>
            <TabsTrigger value="merged">{t('disk_io_merged')}</TabsTrigger>
            <TabsTrigger value="per-disk">{t('disk_io_per_disk')}</TabsTrigger>
          </TabsList>
        </CardHeader>

        <CardContent className="min-w-0">
          <TabsContent value="merged">
            <DiskIoLineChart
              ariaLabel={t('chart_disk_io')}
              data={mergedData}
              formatTime={formatTime}
              series={chartSeries}
              timeLabel={t('chart_time')}
            />
          </TabsContent>

          <TabsContent value="per-disk">
            <div className="space-y-4">
              {perDiskData.map((series) => (
                <div className="min-w-0" key={series.name}>
                  <h4 className="mb-3 font-medium text-sm">{series.name}</h4>
                  <DiskIoLineChart
                    ariaLabel={`${t('chart_disk_io')} · ${series.name}`}
                    data={series.data}
                    formatTime={formatTime}
                    series={chartSeries}
                    timeLabel={t('chart_time')}
                  />
                </div>
              ))}
            </div>
          </TabsContent>
        </CardContent>
      </Tabs>
    </Card>
  )
}
