import { describe, expect, it, vi } from 'vitest'

vi.mock('@tanstack/react-router', () => ({
  createFileRoute: () => (config: Record<string, unknown>) => config,
  redirect: (options: Record<string, unknown>) => ({ ...options, __redirect: true })
}))

const { Route } = await import('./$serverId')

interface RedirectShape {
  __redirect: boolean
  params: { id: string }
  replace: boolean
  search: { range: string; tab: string }
  to: string
}

function runBeforeLoad(range: string): RedirectShape {
  const { beforeLoad } = Route as unknown as {
    beforeLoad: (ctx: { params: { serverId: string }; search: { range: string } }) => never
  }
  try {
    beforeLoad({ params: { serverId: 'server-1' }, search: { range } })
  } catch (thrown) {
    return thrown as RedirectShape
  }
  throw new Error('beforeLoad did not throw a redirect')
}

describe('network detail route redirect', () => {
  it('redirects to the server detail network tab', () => {
    const redirect = runBeforeLoad('realtime')

    expect(redirect.__redirect).toBe(true)
    expect(redirect.to).toBe('/servers/$id')
    expect(redirect.params).toEqual({ id: 'server-1' })
    expect(redirect.search).toEqual({ tab: 'network', range: 'realtime' })
    expect(redirect.replace).toBe(true)
  })

  it('maps legacy hour ranges to metrics-style range keys', () => {
    expect(runBeforeLoad('1').search.range).toBe('1h')
    expect(runBeforeLoad('6').search.range).toBe('6h')
    expect(runBeforeLoad('24').search.range).toBe('24h')
    expect(runBeforeLoad('168').search.range).toBe('7d')
    expect(runBeforeLoad('720').search.range).toBe('30d')
  })

  it('falls back to realtime for unknown range values', () => {
    expect(runBeforeLoad('999').search.range).toBe('realtime')
  })

  it('defaults the range search param to realtime', () => {
    const { validateSearch } = Route as unknown as {
      validateSearch: (search: Record<string, unknown>) => { range: string }
    }

    expect(validateSearch({})).toEqual({ range: 'realtime' })
    expect(validateSearch({ range: '24' })).toEqual({ range: '24' })
  })
})
