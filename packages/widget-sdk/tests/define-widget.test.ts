import { describe, expect, it } from 'vitest'
import { defineWidget } from '../src/define-widget'

describe('defineWidget', () => {
  it('wraps the user input into a WidgetModule shape', () => {
    const mod = defineWidget({
      configSchema: { _kind: 'object', shape: {} } as any,
      component: () => null
    })
    expect(mod.__brand).toBe('WidgetModule')
    expect(typeof mod.component).toBe('function')
    expect(mod.actions).toEqual([])
  })

  it('preserves user-supplied actions array', () => {
    const mod = defineWidget({
      configSchema: { _kind: 'object', shape: {} } as any,
      component: () => null,
      actions: [{ id: 'a', label: 'A', run: async () => {} }]
    })
    expect(mod.actions).toHaveLength(1)
    expect(mod.actions[0].id).toBe('a')
  })

  it('throws when component is missing', () => {
    expect(() => defineWidget({ configSchema: {} as any } as any)).toThrow(/component/)
  })
})
