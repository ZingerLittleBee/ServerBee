import { cleanup, render, screen } from '@testing-library/react'
import type { ResponsiveBones } from 'boneyard-js'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { BoneSkeleton } from './bone-skeleton'

const TEST_BONES: ResponsiveBones = {
  breakpoints: {
    375: {
      name: 'test-surface',
      viewportWidth: 375,
      width: 375,
      height: 24,
      bones: [[0, 0, 100, 24, 4]]
    }
  }
}

function stubMatchMedia(reducedMotion: boolean) {
  window.matchMedia = ((query: string) => ({
    matches: reducedMotion && query === '(prefers-reduced-motion: reduce)',
    media: query,
    onchange: null,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false
  })) as unknown as typeof window.matchMedia
}

describe('BoneSkeleton', () => {
  beforeEach(() => {
    stubMatchMedia(false)
  })

  afterEach(() => {
    cleanup()
  })

  it('marks the named container busy and renders bones while loading', () => {
    const { container } = render(
      <BoneSkeleton initialBones={TEST_BONES} loading name="test-surface">
        <p>Real content</p>
      </BoneSkeleton>
    )

    const el = container.querySelector('[data-boneyard="test-surface"]')
    expect(el).not.toBeNull()
    expect(el?.getAttribute('aria-busy')).toBe('true')
    expect(container.querySelectorAll('[data-boneyard-bone]')).toHaveLength(1)
  })

  it('keeps children mounted but out of the accessibility tree while loading', () => {
    const { container } = render(
      <BoneSkeleton initialBones={TEST_BONES} loading name="test-surface">
        <button type="button">Real action</button>
      </BoneSkeleton>
    )

    // boneyard hides content with visibility:hidden (no aria-hidden). That
    // removes children from the accessibility tree and focus order — assert
    // this effective behavior instead of an attribute upstream does not set.
    const content = container.querySelector('[data-boneyard-content]') as HTMLElement
    expect(content.style.visibility).toBe('hidden')
    // Children stay mounted so the container keeps the loaded page's
    // dimensions (no layout shift when bones resolve)...
    expect(screen.getByText('Real action')).toBeInTheDocument()
    // ...but they are neither visible nor focusable to users.
    expect(screen.queryByRole('button', { name: 'Real action' })).toBeNull()
  })

  it('restores children to the accessibility tree once loading completes', () => {
    const { container } = render(
      <BoneSkeleton initialBones={TEST_BONES} loading={false} name="test-surface">
        <button type="button">Real action</button>
      </BoneSkeleton>
    )

    expect(container.querySelectorAll('[data-boneyard-bone]')).toHaveLength(0)
    expect(container.querySelector('[data-boneyard]')?.getAttribute('aria-busy')).toBeNull()
    const content = container.querySelector('[data-boneyard-content]') as HTMLElement
    expect(content.style.visibility).toBe('')
    expect(screen.getByRole('button', { name: 'Real action' })).toBeInTheDocument()
  })

  it('injects pulse keyframes when motion is allowed', () => {
    const { container } = render(
      <BoneSkeleton initialBones={TEST_BONES} loading name="test-surface">
        <p>Real content</p>
      </BoneSkeleton>
    )

    const styles = Array.from(container.querySelectorAll('style')).map((s) => s.textContent ?? '')
    expect(styles.some((css) => css.includes('@keyframes bp-'))).toBe(true)
  })

  it('renders static bones with no keyframes when the user prefers reduced motion', () => {
    stubMatchMedia(true)
    const { container } = render(
      <BoneSkeleton initialBones={TEST_BONES} loading name="test-surface">
        <p>Real content</p>
      </BoneSkeleton>
    )

    expect(container.querySelectorAll('[data-boneyard-bone]')).toHaveLength(1)
    expect(container.querySelectorAll('style')).toHaveLength(0)
  })

  it('shows the generic visible fallback when the name has no registered bones', () => {
    const { container } = render(
      <BoneSkeleton loading name="unregistered-surface">
        <p>Fixture content</p>
      </BoneSkeleton>
    )

    // Loading semantics stay intact even without bones.
    const el = container.querySelector('[data-boneyard="unregistered-surface"]')
    expect(el).not.toBeNull()
    expect(el?.getAttribute('aria-busy')).toBe('true')

    // The fallback replaces children entirely: fake fixture content never
    // renders, and the page is not left permanently blank.
    expect(screen.queryByText('Fixture content')).toBeNull()
    const fallback = container.querySelector('[data-boneyard-fallback]')
    expect(fallback).not.toBeNull()
    expect(fallback?.getAttribute('aria-hidden')).toBe('true')

    // Generic inert placeholders only: no fake data, nothing focusable.
    expect(fallback?.textContent).toBe('')
    expect(fallback?.querySelectorAll('button, a, input, select, textarea, [tabindex]')).toHaveLength(0)

    // Motion allowed: the fallback keeps its pulse, with the primitive's base
    // class replaced (not stacked) so exactly one animation utility remains.
    const blocks = Array.from(fallback?.querySelectorAll('[data-slot="skeleton"]') ?? [])
    expect(blocks.length).toBeGreaterThan(0)
    for (const block of blocks) {
      expect(block.className).toContain('animate-pulse')
      expect(block.className).not.toContain('animate-none')
    }
  })

  it('renders a fully static fallback for unregistered names under reduced motion', () => {
    stubMatchMedia(true)
    const { container } = render(
      <BoneSkeleton loading name="unregistered-surface">
        <p>Fixture content</p>
      </BoneSkeleton>
    )

    // Loading semantics still hold for the unregistered + reduced-motion mix.
    const el = container.querySelector('[data-boneyard="unregistered-surface"]')
    expect(el).not.toBeNull()
    expect(el?.getAttribute('aria-busy')).toBe('true')
    expect(screen.queryByText('Fixture content')).toBeNull()

    const fallback = container.querySelector('[data-boneyard-fallback]')
    expect(fallback).not.toBeNull()
    expect(fallback?.getAttribute('aria-hidden')).toBe('true')

    // Regression: the shadcn primitive's unconditional base `animate-pulse`
    // must be gone from the merged class list — a coexisting `motion-safe:`
    // variant never wins the cascade, so no pulse/shimmer utility may remain.
    const blocks = Array.from(fallback?.querySelectorAll('[data-slot="skeleton"]') ?? [])
    expect(blocks.length).toBeGreaterThan(0)
    for (const block of blocks) {
      expect(block.className).toContain('animate-none')
      expect(block.className).not.toContain('animate-pulse')
      expect(block.className).not.toContain('motion-safe')
    }
  })
})
