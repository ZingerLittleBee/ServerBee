/**
 * @serverbee-widget {
 *   "id": "com.serverbee.hello-world",
 *   "version": "1.0.0",
 *   "name": "Hello World",
 *   "description": "A minimal builtin widget that displays the SPA theme mode and the count of online servers.",
 *   "author": "ServerBee",
 *   "category": "Real-time",
 *   "sizing": { "defaultW": 3, "defaultH": 2, "minW": 2, "minH": 2, "strategy": "free" },
 *   "sdkVersion": "^0.1.0"
 * }
 */
import { defineWidget, useServers, useTheme, z } from '@serverbee/widget-sdk'

const ConfigSchema = z.object({
  greeting: z.string().describe('Greeting text').default('Hello, ServerBee')
})

export default defineWidget({
  configSchema: ConfigSchema,
  component: ({ config }) => {
    const servers = useServers()
    const theme = useTheme()
    const online = servers.filter((s) => s.online).length
    const { greeting } = config as { greeting: string }
    return (
      <div style={{ padding: 12, fontFamily: 'system-ui' }}>
        <div style={{ fontSize: 16, fontWeight: 600 }}>{greeting}</div>
        <div style={{ marginTop: 8, color: 'var(--muted-foreground)' }}>
          {online} / {servers.length} online · {theme.mode} mode
        </div>
      </div>
    )
  }
})
