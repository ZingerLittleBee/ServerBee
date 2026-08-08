import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { cleanup, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, to, ...rest }: { children: ReactNode; to: string }) => (
    <a href={to} {...rest}>
      {children}
    </a>
  )
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ i18n: { language: 'en' }, t: (key: string) => key })
}))

const CAPTURE_NAMES = [
  'status-overview-grid',
  'status-overview-list',
  'status-server-detail',
  'status-network-detail',
  'server-detail',
  'traffic-overview',
  'service-monitor-detail'
]

describe('boneyard capture surface', () => {
  afterEach(() => {
    cleanup()
  })

  it('is not registered in the committed production route tree', () => {
    // The capture surface mounts via a dev-only branch in the root layout,
    // not a file route — the generated route tree must not know the path.
    const routeTree = readFileSync(resolve(import.meta.dirname, '../../routeTree.gen.ts'), 'utf8')
    expect(routeTree).not.toContain('boneyard-capture')
  })

  it('keeps the capture-page import expression behind the compile-time DEV gate', () => {
    // Rollup can only tree-shake the capture chunk when the import()
    // expression itself is unreachable in prod — a top-level lazy import
    // outside the import.meta.env.DEV gate stays reachable and ships.
    const source = readFileSync(resolve(import.meta.dirname, '../../routes/__root.tsx'), 'utf8')
    const importIndex = source.indexOf("import('@/components/boneyard/capture-page')")
    expect(importIndex).toBeGreaterThan(-1)
    const declarationIndex = source.lastIndexOf('const BoneyardCapturePage', importIndex)
    expect(declarationIndex).toBeGreaterThan(-1)
    expect(source.slice(declarationIndex, importIndex)).toContain('import.meta.env.DEV')
  })

  it('still renders every named fixture for the explicit generation workflow', async () => {
    // Simulate the CLI's build mode so Skeleton renders fixtures for capture.
    ;(window as unknown as Record<string, unknown>).__BONEYARD_BUILD = true
    try {
      const { BoneyardCapturePage } = await import('./capture-page')
      const { container } = render(<BoneyardCapturePage />)

      for (const name of CAPTURE_NAMES) {
        expect(container.querySelector(`[data-boneyard="${name}"]`), name).not.toBeNull()
      }
      // Fixtures really rendered (deterministic fake content, not the
      // unregistered-bones fallback).
      expect(screen.getAllByText('tokyo-edge-01').length).toBeGreaterThan(0)
      expect(container.querySelector('[data-boneyard-fallback]')).toBeNull()
    } finally {
      ;(window as unknown as Record<string, unknown>).__BONEYARD_BUILD = undefined
    }
  })
})
