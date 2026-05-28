import type { WidgetManifest, WidgetModule } from '@serverbee/widget-sdk'
import { beforeEach, describe, expect, it } from 'vitest'
import { registryActions, useWidgetRegistry } from './registry'

const fakeManifest: WidgetManifest = {
  id: 'com.test.foo',
  version: '1.0.0',
  name: 'Foo',
  category: 'Real-time',
  sizing: { defaultW: 2, defaultH: 2, minW: 1, minH: 1, strategy: 'free' },
  sdkVersion: '^0.1.0'
}
const fakeModule: WidgetModule = {
  __brand: 'WidgetModule',
  configSchema: {} as any,
  component: () => null,
  actions: []
}

describe('registry', () => {
  beforeEach(() => useWidgetRegistry.setState({ modules: new Map(), failures: new Map() }))

  it('registers and retrieves a module', () => {
    registryActions.register('com.test.foo', fakeModule, fakeManifest)
    const entry = registryActions.get('com.test.foo')
    expect(entry?.manifest.name).toBe('Foo')
    expect(entry?.module).toBe(fakeModule)
  })

  it('records load failures', () => {
    registryActions.recordLoadFailure('com.test.bad', new Error('boom'))
    expect(registryActions.list()).toEqual([])
    expect(useWidgetRegistry.getState().failures.get('com.test.bad')?.message).toBe('boom')
  })

  it('unregister removes the module', () => {
    registryActions.register('com.test.foo', fakeModule, fakeManifest)
    registryActions.unregister('com.test.foo')
    expect(registryActions.get('com.test.foo')).toBeUndefined()
  })
})
