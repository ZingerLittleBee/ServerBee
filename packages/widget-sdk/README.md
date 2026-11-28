# @serverbee/widget-sdk

Stable TypeScript SDK for authoring ServerBee custom dashboard widgets.

For the full authoring, packaging, installation, and security guide, see [Custom Widgets](../../apps/docs/content/docs/en/custom-widgets.mdx) ([Chinese](../../apps/docs/content/docs/zh/custom-widgets.mdx)).

## Entry points

- `@serverbee/widget-sdk` — main barrel re-exporting all runtime helpers, hooks, schema builders, actions, and types.
- `@serverbee/widget-sdk/z` — bundled Zod-compatible schema builder and custom field extensions.
- `@serverbee/widget-sdk/hooks` — live, domain, host, and escape-hatch React hooks.
- `@serverbee/widget-sdk/actions` — `createActionsHelper` and action types.

## Core exports

- `SDK_VERSION`: current SDK semver string (`"0.1.0"`).
- `defineWidget({ configSchema, component, actions? })`: declares a `WidgetModule`.
  - `component` receives `WidgetComponentProps<TConfig>`: `{ config, size: { w, h }, isEditing, actions }`.
  - `actions` is an optional `ActionDefinition[]` rendered via `actions.render(id)`.
- `validateManifest(input)` / `isCompatible(hostVersion, range)`: manifest validation and semver compatibility helpers.
- `ConfigForm` / `renderConfigForm(schema, value, onChange)`: auto-generated configuration form renderer.
- `createActionsHelper(actions)`: builds an `ActionsHelper` (`{ render(id) }`) backed by `ActionButton` (confirmation dialog, pending state, and toast notifications).
- `createWidgetRuntime` / `getRuntime` / `resetRuntime`: host bridge lifecycle helpers.

## Schema builder (`z`)

Built-in primitives plus ServerBee field extensions:

- `z.serverId()` — non-empty server identifier picker.
- `z.metricPath()` — dot/bracket metric path (e.g. `cpu.usage`, `disks[0].used`).
- `z.color()` — CSS color (`#hex`, `oklch(...)`, `rgb(...)`, `hsl(...)`).
- `z.duration()` — duration string matching `^\d+(s|m|h|d)$` (e.g. `5m`, `1h`, `24h`, `7d`).
- Types: `ZodSchema`, `ZodTypeAny`, `Infer`, `ZError`.

## Hooks

### Live hooks

- `useServers(): ServerSummary[]`
- `useServer(id: string | null): unknown`
- `useMetric(id: string | null, path: string): number | string | undefined`
- `useCapability(id: string | null, cap: string): boolean`
  - Supported capability keys: `CAP_TERMINAL`, `CAP_EXEC`, `CAP_UPGRADE`, `CAP_PING_ICMP`, `CAP_PING_TCP`, `CAP_PING_HTTP`, `CAP_FILE`, `CAP_DOCKER`, `CAP_SECURITY_EVENTS`, `CAP_FIREWALL_BLOCK`, `CAP_IP_QUALITY`, `CAP_TAILSCALE`.

### Domain hooks

- `useHistory(serverId: string | null, field: string, range = '1h', opts?: { interval?: 'auto' | 'raw' | 'hourly' }): UseQueryResult<HistoryPoint[]>`
  - Queries `GET /api/servers/{id}/records?from=<ISO>&to=<ISO>&interval=<auto|raw|hourly>` and maps records to `{ ts: number, value: number }[]`.
  - Supported `field` keys: `cpu`, `mem_used`, `swap_used`, `disk_used`, `net_in_speed` (alias `net_in`), `net_out_speed` (alias `net_out`), `network` (`net_in_speed + net_out_speed`), `net_in_transfer`, `net_out_transfer`, `load1`, `load5`, `load15`, `tcp_conn`, `udp_conn`, `process_count`, `temperature`, `gpu_usage`.
- `useTraffic(serverId: string | null, range?: string)` — queries `/api/servers/{id}/traffic` (or `/api/traffic/overview/daily` when `serverId` is `null`).
- `useUptime(serverId: string | null, days = 30)` — queries `/api/servers/{id}/uptime-daily`.
- `useAlerts(opts?: { limit?: number })` — queries `/api/alert-events`.
- `useServiceMonitors()` — queries `/api/service-monitors`.
- `useGeoIp()` — returns `{ status, download }` for `/api/geoip/status` and `POST /api/geoip/download`.

### Host and escape-hatch hooks

- `useTheme(): ThemeSnapshot` (`{ mode: 'light' | 'dark', cssVar(name) }`)
- `useConfigUpdate<TConfig>(instanceId: string)`
- `useApiQuery<T>(path, opts?)`
- `useApiMutation<TRes, TReq>(method, path, opts?)`
