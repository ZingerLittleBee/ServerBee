import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { cleanup, render } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
// Side-effect import: runs configureBoneyard + registerBones exactly like the
// app entry (src/main.tsx) does.
import '@/bones/registry'
import serverDetail from '@/bones/server-detail.bones.json'
import serviceMonitorDetail from '@/bones/service-monitor-detail.bones.json'
import statusNetworkDetail from '@/bones/status-network-detail.bones.json'
import statusOverviewGrid from '@/bones/status-overview-grid.bones.json'
import statusOverviewList from '@/bones/status-overview-list.bones.json'
import statusServerDetail from '@/bones/status-server-detail.bones.json'
import trafficOverview from '@/bones/traffic-overview.bones.json'
import { BoneSkeleton } from './bone-skeleton'

const GENERATED = {
  'status-overview-grid': statusOverviewGrid,
  'status-overview-list': statusOverviewList,
  'status-server-detail': statusServerDetail,
  'status-network-detail': statusNetworkDetail,
  'server-detail': serverDetail,
  'traffic-overview': trafficOverview,
  'service-monitor-detail': serviceMonitorDetail
} as const

const EXPECTED_BREAKPOINTS = ['375', '768', '1024', '1280']

describe('generated bones registry', () => {
  afterEach(() => {
    cleanup()
    document.documentElement.classList.remove('dark')
  })

  it('captures every major loading surface at the configured breakpoints', () => {
    for (const [name, data] of Object.entries(GENERATED)) {
      // Breakpoint keys are strings ("375", "1024", …): sort numerically,
      // since the default lexicographic order scrambles magnitudes.
      expect(Object.keys(data.breakpoints).sort((a, b) => Number(a) - Number(b)), name).toEqual(EXPECTED_BREAKPOINTS)
      for (const result of Object.values(data.breakpoints)) {
        expect(result.bones.length, name).toBeGreaterThan(0)
      }
    }
  })

  it('stores bone geometry only — no text, HTML, or production data', () => {
    for (const [name, data] of Object.entries(GENERATED)) {
      for (const result of Object.values(data.breakpoints)) {
        expect(result.name).toBe(name)
        expect(typeof result.width).toBe('number')
        expect(typeof result.height).toBe('number')
        for (const bone of result.bones) {
          expect(Array.isArray(bone)).toBe(true)
          expect(bone.length).toBeGreaterThanOrEqual(5)
          expect(bone.length).toBeLessThanOrEqual(6)
          // [x, y, width, height, radius, isCard?] — numbers plus an optional
          // radius string like "50%" and the card flag.
          expect(typeof bone[0]).toBe('number')
          expect(typeof bone[1]).toBe('number')
          expect(typeof bone[2]).toBe('number')
          expect(typeof bone[3]).toBe('number')
          expect(['number', 'string']).toContain(typeof bone[4])
          if (bone.length === 6) {
            expect(typeof bone[5]).toBe('boolean')
          }
        }
      }
      // Fixture copy uses distinctive fake names; none may leak into the
      // generated artifacts (proves the capture is geometry-only).
      const serialized = JSON.stringify(data)
      expect(serialized).not.toMatch(/fixture-|tokyo|fra-core|db-replica|cache-eu/i)
    }
  })

  it('commits the CLI registry template with configureBoneyard intact', () => {
    // The vite plugin's auto-capture writes a register-only template; the
    // authoritative generation path (`bun run generate:bones`) must be what
    // lands in the committed file, including the token-derived config call.
    const source = readFileSync(resolve(import.meta.dirname, '../../bones/registry.ts'), 'utf8')
    expect(source).toContain('configureBoneyard(')
    expect(source).toContain('"color":"#f5f5f5"')
    expect(source).toContain('"darkColor":"#262626"')
  })

  it('resolves registered bones by name and applies the configured token colors', () => {
    const { container } = render(
      <BoneSkeleton loading name="traffic-overview">
        <div />
      </BoneSkeleton>
    )

    const bones = container.querySelectorAll('[data-boneyard-bone]')
    expect(bones.length).toBeGreaterThan(0)
    // configureBoneyard from the generated registry maps --muted (#f5f5f5).
    const first = bones[0] as HTMLElement
    expect(first.style.backgroundColor).toBe('rgb(245, 245, 245)')
  })

  it('switches bones to the configured dark color when the dark theme is active', () => {
    document.documentElement.classList.add('dark')
    const { container } = render(
      <BoneSkeleton loading name="traffic-overview">
        <div />
      </BoneSkeleton>
    )

    const bones = container.querySelectorAll('[data-boneyard-bone]')
    expect(bones.length).toBeGreaterThan(0)
    // darkColor from configureBoneyard maps --muted dark (#262626).
    const first = bones[0] as HTMLElement
    expect(first.style.backgroundColor).toBe('rgb(38, 38, 38)')
  })
})
