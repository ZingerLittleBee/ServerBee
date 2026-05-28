import { beforeEach, describe, expect, it, vi } from 'vitest'
import { bootstrapLoader } from './loader'
import { useWidgetRegistry } from './registry'

const SDK_VERSION_MISMATCH_RE = /sdk version mismatch/

describe('bootstrapLoader', () => {
  beforeEach(() => {
    useWidgetRegistry.setState({ modules: new Map(), failures: new Map() })
  })

  it('lists modules, imports each, registers them', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        data: [
          {
            id: 'com.test.foo',
            version: '1.0.0',
            entry_path: 'index.js',
            manifest: {
              id: 'com.test.foo',
              version: '1.0.0',
              name: 'Foo',
              category: 'Real-time',
              sizing: { defaultW: 2, defaultH: 2, minW: 1, minH: 1, strategy: 'free' },
              sdkVersion: '^0.1.0'
            }
          }
        ]
      })
    }) as any
    const fakeModule = {
      __brand: 'WidgetModule',
      configSchema: {},
      component: () => null,
      actions: []
    }
    const importer = vi.fn().mockResolvedValue({ default: fakeModule })
    await bootstrapLoader({ importer })
    expect(useWidgetRegistry.getState().modules.size).toBe(1)
    expect(useWidgetRegistry.getState().modules.get('com.test.foo')?.manifest.name).toBe('Foo')
  })

  it('rejects modules with incompatible sdkVersion range', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        data: [
          {
            id: 'com.test.future',
            version: '1.0.0',
            entry_path: 'index.js',
            manifest: {
              id: 'com.test.future',
              version: '1.0.0',
              name: 'Future',
              category: 'Real-time',
              sizing: { defaultW: 2, defaultH: 2, minW: 1, minH: 1, strategy: 'free' },
              // Host is 0.1.0; ^2.0.0 cannot be satisfied.
              sdkVersion: '^2.0.0'
            }
          }
        ]
      })
    }) as any
    const importer = vi.fn()
    await bootstrapLoader({ importer, hostSdkVersion: '0.1.0' })
    expect(importer).not.toHaveBeenCalled()
    expect(useWidgetRegistry.getState().modules.has('com.test.future')).toBe(false)
    const failure = useWidgetRegistry.getState().failures.get('com.test.future')
    expect(failure?.message).toMatch(SDK_VERSION_MISMATCH_RE)
  })

  it('isolates one module failure', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        data: [
          {
            id: 'a',
            version: '1.0.0',
            entry_path: 'a.js',
            manifest: {
              id: 'a',
              version: '1.0.0',
              name: 'A',
              category: 'Real-time',
              sizing: { defaultW: 2, defaultH: 2, minW: 1, minH: 1, strategy: 'free' },
              sdkVersion: '^0.1.0'
            }
          },
          {
            id: 'b',
            version: '1.0.0',
            entry_path: 'b.js',
            manifest: {
              id: 'b',
              version: '1.0.0',
              name: 'B',
              category: 'Real-time',
              sizing: { defaultW: 2, defaultH: 2, minW: 1, minH: 1, strategy: 'free' },
              sdkVersion: '^0.1.0'
            }
          }
        ]
      })
    }) as any
    const importer = vi.fn().mockImplementation((url: string) =>
      url.includes('a.js')
        ? Promise.resolve({
            default: {
              __brand: 'WidgetModule',
              configSchema: {},
              component: () => null,
              actions: []
            }
          })
        : Promise.reject(new Error('boom'))
    )
    await bootstrapLoader({ importer })
    expect(useWidgetRegistry.getState().modules.has('a')).toBe(true)
    expect(useWidgetRegistry.getState().failures.has('b')).toBe(true)
  })
})
