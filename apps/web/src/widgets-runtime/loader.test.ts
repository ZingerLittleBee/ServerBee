import { defineWidget, type WidgetManifest, z } from '@serverbee/widget-sdk'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { bootstrapLoader } from './loader'
import { useWidgetRegistry } from './registry'

const SDK_VERSION_MISMATCH_RE = /sdk version mismatch/

interface TestListEntry {
  entry_path: string
  id: string
  manifest: WidgetManifest
  version: string
}

function mockFetchModules(entries: TestListEntry[]) {
  const fetchMock = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>()
  fetchMock.mockResolvedValue(
    new Response(JSON.stringify({ data: entries }), {
      headers: { 'Content-Type': 'application/json' },
      status: 200
    })
  )
  global.fetch = fetchMock
}

function manifest(id: string, overrides: Partial<WidgetManifest> = {}): WidgetManifest {
  return {
    id,
    version: '1.0.0',
    name: id,
    category: 'Real-time',
    sizing: { defaultW: 2, defaultH: 2, minW: 1, minH: 1, strategy: 'free' },
    sdkVersion: '^0.1.0',
    ...overrides
  }
}

function moduleEntry(id: string, entryPath: string, manifestOverrides: Partial<WidgetManifest> = {}): TestListEntry {
  return {
    id,
    version: '1.0.0',
    entry_path: entryPath,
    manifest: manifest(id, manifestOverrides)
  }
}

function fakeModule() {
  return defineWidget({
    configSchema: z.object({}),
    component: () => null
  })
}

describe('bootstrapLoader', () => {
  beforeEach(() => {
    useWidgetRegistry.setState({ modules: new Map(), failures: new Map() })
  })

  it('lists modules, imports each, registers them', async () => {
    mockFetchModules([moduleEntry('com.test.foo', 'index.js', { name: 'Foo' })])
    const importer = vi.fn().mockResolvedValue({ default: fakeModule() })
    await bootstrapLoader({ importer })
    expect(useWidgetRegistry.getState().modules.size).toBe(1)
    expect(useWidgetRegistry.getState().modules.get('com.test.foo')?.manifest.name).toBe('Foo')
  })

  it('imports nested entry paths by the asset basename exposed by the server route', async () => {
    mockFetchModules([moduleEntry('com.test.nested', 'nested/index.js', { name: 'Nested' })])
    const importer = vi.fn().mockResolvedValue({ default: fakeModule() })

    await bootstrapLoader({ importer })

    expect(importer).toHaveBeenCalledWith('/api/widget-modules/com.test.nested/index.js')
  })

  it('rejects modules with incompatible sdkVersion range', async () => {
    mockFetchModules([
      moduleEntry('com.test.future', 'index.js', {
        name: 'Future',
        sdkVersion: '^2.0.0'
      })
    ])
    const importer = vi.fn()
    await bootstrapLoader({ importer, hostSdkVersion: '0.1.0' })
    expect(importer).not.toHaveBeenCalled()
    expect(useWidgetRegistry.getState().modules.has('com.test.future')).toBe(false)
    const failure = useWidgetRegistry.getState().failures.get('com.test.future')
    expect(failure?.message).toMatch(SDK_VERSION_MISMATCH_RE)
  })

  it('isolates one module failure', async () => {
    mockFetchModules([moduleEntry('a', 'a.js', { name: 'A' }), moduleEntry('b', 'b.js', { name: 'B' })])
    const importer = vi
      .fn()
      .mockImplementation((url: string) =>
        url.includes('a.js') ? Promise.resolve({ default: fakeModule() }) : Promise.reject(new Error('boom'))
      )
    await bootstrapLoader({ importer })
    expect(useWidgetRegistry.getState().modules.has('a')).toBe(true)
    expect(useWidgetRegistry.getState().failures.has('b')).toBe(true)
  })
})
