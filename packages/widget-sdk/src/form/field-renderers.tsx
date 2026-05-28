import { getRuntime } from '../runtime-context'
import type { ZodTypeAny } from '../z'

interface FieldProps {
  id: string
  label: string
  onChange: (v: unknown) => void
  schema: ZodTypeAny
  value: unknown
}

const METRIC_PATH_SUGGESTIONS = [
  'cpu.total',
  'cpu_cores',
  'memory.used_pct',
  'mem_used',
  'mem_total',
  'load1',
  'load5',
  'load15',
  'net_in_speed',
  'net_out_speed',
  'disk_used',
  'disk_total'
]

function getMetricPathSuggestions(): string[] {
  try {
    const rt = getRuntime() as ReturnType<typeof getRuntime> & {
      getMetricPaths?: () => string[]
    }
    if (typeof rt.getMetricPaths === 'function') {
      const fromHost = rt.getMetricPaths()
      if (Array.isArray(fromHost) && fromHost.length > 0) {
        return fromHost
      }
    }
  } catch {
    // runtime not installed — fall back to defaults
  }
  return METRIC_PATH_SUGGESTIONS
}

export function renderField(props: FieldProps) {
  const info = props.schema.introspect()
  switch (info.kind) {
    case 'string':
      return (
        <input
          id={props.id}
          onChange={(e) => props.onChange(e.target.value)}
          type="text"
          value={(props.value as string) ?? ''}
        />
      )
    case 'number':
      return (
        <input
          id={props.id}
          onChange={(e) => props.onChange(e.target.value === '' ? undefined : Number(e.target.value))}
          type="number"
          value={(props.value as number) ?? ''}
        />
      )
    case 'boolean':
      return (
        <input
          checked={!!props.value}
          id={props.id}
          onChange={(e) => props.onChange(e.target.checked)}
          type="checkbox"
        />
      )
    case 'enum': {
      const opts = (info.values ?? []) as readonly string[]
      return (
        <select id={props.id} onChange={(e) => props.onChange(e.target.value)} value={(props.value as string) ?? ''}>
          {opts.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>
      )
    }
    case 'serverId': {
      const servers = getRuntime().serversStore()
      return (
        <select id={props.id} onChange={(e) => props.onChange(e.target.value)} value={(props.value as string) ?? ''}>
          <option value="">— choose —</option>
          {servers.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>
      )
    }
    case 'metricPath': {
      const suggestions = getMetricPathSuggestions()
      const listId = `${props.id}-paths`
      return (
        <>
          <input
            id={props.id}
            list={listId}
            onChange={(e) => props.onChange(e.target.value)}
            placeholder="cpu.total"
            type="text"
            value={(props.value as string) ?? ''}
          />
          <datalist id={listId}>
            {suggestions.map((path) => (
              <option key={path} value={path} />
            ))}
          </datalist>
        </>
      )
    }
    case 'color':
      return (
        <input
          id={props.id}
          onChange={(e) => props.onChange(e.target.value)}
          type="color"
          value={(props.value as string) ?? '#000000'}
        />
      )
    case 'duration':
      return (
        <input
          id={props.id}
          onChange={(e) => props.onChange(e.target.value)}
          pattern="^\d+(s|m|h|d)$"
          placeholder="30s"
          title="Examples: 30s, 5m, 1h, 7d"
          type="text"
          value={(props.value as string) ?? ''}
        />
      )
    default:
      return (
        <input
          id={props.id}
          onChange={(e) => props.onChange(e.target.value)}
          type="text"
          value={(props.value as string) ?? ''}
        />
      )
  }
}
