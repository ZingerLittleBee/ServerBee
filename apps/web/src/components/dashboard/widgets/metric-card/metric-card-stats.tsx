interface StatProps {
  caption: string
  kind: 'peak' | 'avg'
  value: string
}

function Stat({ caption, kind, value }: StatProps) {
  return (
    <div className="flex-1 rounded-md border bg-muted/40 px-2.5 py-1.5">
      <p className="font-medium text-[0.625rem] text-muted-foreground uppercase tracking-[0.12em]">{caption}</p>
      <p className="font-semibold text-sm tabular-nums leading-tight" data-testid={`metric-card-stat-${kind}`}>
        {value}
      </p>
    </div>
  )
}

interface MetricCardStatsProps {
  avg: string
  avgCaption: string
  peak: string
  peakCaption: string
}

export function MetricCardStats({ peakCaption, avgCaption, peak, avg }: MetricCardStatsProps) {
  return (
    <div className="flex gap-2">
      <Stat caption={peakCaption} kind="peak" value={peak} />
      <Stat caption={avgCaption} kind="avg" value={avg} />
    </div>
  )
}
