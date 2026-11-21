import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { validateManifest } from '@serverbee/widget-sdk'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import tailscaleWidget from './tailscale-overview.widget'

const JSDOC_RE = /\/\*\*[\s\S]*?@serverbee-widget\s+(\{[\s\S]*?\})\s*\*\//
const LINE_DECOR = /^\s*\*\s?/gm

describe('tailscale-overview builtin widget', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('declares a valid @serverbee-widget manifest header', () => {
    const src = readFileSync(resolve(import.meta.dirname, 'tailscale-overview.widget.tsx'), 'utf8')
    const match = src.match(JSDOC_RE)
    expect(match).not.toBeNull()
    const rawManifest = JSON.parse((match?.[1] ?? '{}').replace(LINE_DECOR, ''))
    const validated = validateManifest(rawManifest)
    expect(validated.id).toBe('com.serverbee.tailscale-overview')
    expect(validated.category).toBe('Status')
  })

  it('fetches /api/tailscale/overview via widget SDK and displays online/total devices, linked servers, and expiring keys', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          data: {
            configured: true,
            provider: 'tailscale',
            tailnet: 'prod.ts.net',
            total_devices: 12,
            online_devices: 9,
            linked_servers: 5,
            expiring_keys_count: 2
          }
        }),
        { headers: { 'Content-Type': 'application/json' }, status: 200 }
      )
    )

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } }
    })
    const Component = tailscaleWidget.component

    render(
      <QueryClientProvider client={queryClient}>
        <Component
          actions={{ render: () => null }}
          config={{ title: 'Tailscale Mesh' }}
          isEditing={false}
          size={{ w: 4, h: 2 }}
        />
      </QueryClientProvider>
    )

    await waitFor(() => expect(screen.getByText('9 / 12')).toBeInTheDocument())
    expect(fetchSpy).toHaveBeenCalledWith('/api/tailscale/overview', expect.objectContaining({ method: 'GET' }))
    expect(screen.getByText('Tailscale Mesh')).toBeInTheDocument()
    expect(screen.getByText('prod.ts.net')).toBeInTheDocument()
    expect(screen.getByText('5')).toBeInTheDocument()
    expect(screen.getByText('2')).toBeInTheDocument()
  })
})
