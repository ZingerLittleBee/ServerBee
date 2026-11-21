/**
 * @serverbee-widget {
 *   "id": "com.serverbee.tailscale-overview",
 *   "version": "1.0.0",
 *   "name": "Tailscale Overview",
 *   "description": "Displays Tailnet online/total devices, linked ServerBee servers, and expiring keys.",
 *   "author": "ServerBee",
 *   "category": "Status",
 *   "sizing": { "defaultW": 4, "defaultH": 2, "minW": 3, "minH": 2, "strategy": "free" },
 *   "sdkVersion": "^0.1.0"
 * }
 */
import { defineWidget, type Infer, useApiQuery, type WidgetComponentProps, z } from '@serverbee/widget-sdk'

interface TailnetWidgetOverview {
  configured: boolean
  error?: string | null
  expiring_keys_count: number
  linked_servers: number
  online_devices: number
  provider: string
  tailnet: string
  total_devices: number
}

const ConfigSchema = z.object({
  title: z.string().describe('Widget title').default('Tailscale')
})

type TailscaleWidgetConfig = Infer<typeof ConfigSchema>

function StatItem({ highlight, label, value }: { highlight?: boolean; label: string; value: string }) {
  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: 2,
        padding: '8px 10px',
        borderRadius: 8,
        border: '1px solid var(--border)',
        backgroundColor: 'var(--muted)'
      }}
    >
      <span style={{ fontSize: 11, color: 'var(--muted-foreground)' }}>{label}</span>
      <span
        style={{
          fontSize: 18,
          fontWeight: 600,
          fontVariantNumeric: 'tabular-nums',
          color: highlight ? '#d97706' : 'var(--foreground)'
        }}
      >
        {value}
      </span>
    </div>
  )
}

function TailscaleOverviewWidget({ config }: WidgetComponentProps<TailscaleWidgetConfig>) {
  const { data, isLoading, isError } = useApiQuery<TailnetWidgetOverview>('/api/tailscale/overview')

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        justifyContent: 'space-between',
        height: '100%',
        padding: 12,
        fontFamily: 'system-ui, sans-serif',
        color: 'var(--foreground)'
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
        <span style={{ fontSize: 14, fontWeight: 600 }}>{config.title}</span>
        {data?.configured && (
          <span
            style={{
              fontSize: 11,
              color: 'var(--muted-foreground)',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap'
            }}
          >
            {data.tailnet || data.provider}
          </span>
        )}
      </div>

      {isLoading && <div style={{ fontSize: 12, color: 'var(--muted-foreground)' }}>Loading Tailnet overview…</div>}

      {!isLoading && (isError || data?.error) && (
        <div style={{ fontSize: 12, color: 'var(--destructive, #dc2626)' }}>
          {data?.error || 'Failed to load Tailnet overview'}
        </div>
      )}

      {!(isLoading || isError || data?.error || data?.configured) && (
        <div style={{ fontSize: 12, color: 'var(--muted-foreground)' }}>
          Tailscale Control Plane integration is not configured.
        </div>
      )}

      {!(isLoading || isError || data?.error) && data?.configured && (
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(3, minmax(0, 1fr))',
            gap: 8,
            marginTop: 8
          }}
        >
          <StatItem label="Online / Total" value={`${data.online_devices} / ${data.total_devices}`} />
          <StatItem label="Linked Servers" value={String(data.linked_servers)} />
          <StatItem
            highlight={data.expiring_keys_count > 0}
            label="Expiring Keys"
            value={String(data.expiring_keys_count)}
          />
        </div>
      )}
    </div>
  )
}

export default defineWidget({
  configSchema: ConfigSchema,
  component: TailscaleOverviewWidget
})
