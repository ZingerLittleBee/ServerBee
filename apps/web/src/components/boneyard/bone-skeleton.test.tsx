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

  it('keeps children mounted but hidden from paint and assistive tech while loading', () => {
    const { container } = render(
      <BoneSkeleton initialBones={TEST_BONES} loading name="test-surface">
        <p>Real content</p>
      </BoneSkeleton>
    )

    const content = container.querySelector('[data-boneyard-content]') as HTMLElement
    expect(content.getAttribute('aria-hidden')).toBe('true')
    expect(content.style.visibility).toBe('hidden')
    // Children stay mounted so the container keeps the loaded page's
    // dimensions (no layout shift when bones resolve).
    expect(screen.getByText('Real content')).toBeInTheDocument()
  })

  it('renders children instead of bones once loading completes', () => {
    const { container } = render(
      <BoneSkeleton initialBones={TEST_BONES} loading={false} name="test-surface">
        <p>Real content</p>
      </BoneSkeleton>
    )

    expect(container.querySelectorAll('[data-boneyard-bone]')).toHaveLength(0)
    expect(container.querySelector('[data-boneyard]')?.getAttribute('aria-busy')).toBeNull()
    const content = container.querySelector('[data-boneyard-content]') as HTMLElement
    expect(content.style.visibility).toBe('')
    expect(content.getAttribute('aria-hidden')).toBeNull()
    expect(screen.getByText('Real content')).toBeInTheDocument()
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

  it('renders nothing while loading when the name has no registered bones', () => {
    const { container } = render(
      <BoneSkeleton loading name="unregistered-surface">
        <p>Fixture content</p>
      </BoneSkeleton>
    )

    // fallback defaults to null: fake fixture content must never flash when
    // bones are missing (e.g. registry not regenerated yet).
    expect(container.querySelector('[data-boneyard="unregistered-surface"]')).not.toBeNull()
    expect(container.querySelectorAll('[data-boneyard-bone]')).toHaveLength(0)
    expect(screen.queryByText('Fixture content')).toBeNull()
  })
})
