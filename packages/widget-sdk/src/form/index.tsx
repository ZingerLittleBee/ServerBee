import type { ReactNode } from 'react'
import type { ZodTypeAny } from '../z'
import { renderField } from './field-renderers'

export function renderConfigForm(
  schema: ZodTypeAny,
  value: Record<string, unknown>,
  onChange: (v: Record<string, unknown>) => void
): ReactNode {
  if ((schema as any)._kind !== 'object') {
    return <em>Top-level schema must be z.object()</em>
  }
  const shape = (schema as any).shape as Record<string, ZodTypeAny>
  return (
    <div>
      {Object.entries(shape).map(([key, fieldSchema]) => {
        const label = (fieldSchema as any)._label ?? key
        const id = `cfg-${key}`
        return (
          <div key={key} style={{ marginBottom: 8 }}>
            <label htmlFor={id} style={{ display: 'block' }}>
              {label}
            </label>
            {renderField({
              schema: fieldSchema,
              value: value[key],
              onChange: (v) => onChange({ ...value, [key]: v }),
              id,
              label
            })}
          </div>
        )
      })}
    </div>
  )
}
