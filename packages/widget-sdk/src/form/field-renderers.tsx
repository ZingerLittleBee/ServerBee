import { getRuntime } from '../runtime-context'
import type { ZodTypeAny } from '../z'

interface FieldProps {
  id: string
  label: string
  onChange: (v: unknown) => void
  schema: ZodTypeAny
  value: unknown
}

export function renderField(props: FieldProps) {
  const kind = (props.schema as any)._kind
  switch (kind) {
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
      const opts = (props.schema as any).values as string[]
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
    default:
      // metricPath / color / duration → text input
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
