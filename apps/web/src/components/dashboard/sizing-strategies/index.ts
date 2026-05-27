import type { LayoutItem } from 'react-grid-layout'
import type { LayoutConstraint, ResizeHandleAxis } from 'react-grid-layout/core'
import { aspectRatio } from 'react-grid-layout/core'
import type { SizingStrategy } from '@/lib/widget-types'
import { SCALE } from '../grid-constants'

// Returns the tier closest to `value`. Ties pick the first (smaller) tier —
// conservative bias avoids accidental upgrades when the user releases mid-drag.
export function nearestTier(value: number, tiers: readonly number[]): number {
  return tiers.reduce((best, t) => (Math.abs(t - value) < Math.abs(best - value) ? t : best), tiers[0])
}

// A coarse-unit override returned by `snapOnRelease`. The grid layer applies
// it to fine-unit layout items via `applyCoarsePatch`.
export interface SnapPatch {
  h?: number // coarse units (will be multiplied by SCALE when applied)
  w?: number // coarse units
}

export function applyCoarsePatch(item: LayoutItem, patch: SnapPatch): LayoutItem {
  return {
    ...item,
    w: patch.w ?? item.w,
    h: patch.h !== undefined ? patch.h * SCALE : item.h
  }
}

export interface StrategyDescriptor {
  // Constraints attached to LayoutItem.constraints. Applied by RGL inside
  // applySizeConstraints during the resize pipeline. NOT applied at idle
  // layout sync — use `normalizeRenderItem` for idle h.
  constraints: LayoutConstraint[]

  // Whether the item participates in resize at all.
  isResizable: boolean

  // Which resize handles to render. undefined = RGL default (all 8 corners/edges).
  resizeHandles?: ResizeHandleAxis[]
}

function lockHeight(fineH: number): LayoutConstraint {
  return {
    name: `lockHeight(${fineH})`,
    constrainSize(_item, w) {
      return { w, h: fineH }
    }
  }
}

export interface SnapContext {
  containerWidth: number
}

export function snapOnRelease(item: LayoutItem, strategy: SizingStrategy, _ctx: SnapContext): SnapPatch {
  switch (strategy.kind) {
    case 'aspect-square': {
      const tier = nearestTier(item.w, strategy.tiers)
      return { w: tier, h: tier }
    }
    case 'free':
    case 'fixed':
    case 'content-height':
      return {}
  }
}

export function applyStrategy(strategy: SizingStrategy, measuredFineH?: number): StrategyDescriptor {
  switch (strategy.kind) {
    case 'free':
      return { constraints: [], resizeHandles: undefined, isResizable: true }
    case 'fixed':
      return { constraints: [], resizeHandles: [], isResizable: false }
    case 'aspect-square':
      return { constraints: [aspectRatio(1)], resizeHandles: ['se'], isResizable: true }
    case 'content-height':
      return {
        constraints: measuredFineH !== undefined ? [lockHeight(measuredFineH)] : [],
        resizeHandles: ['e'],
        isResizable: true
      }
  }
}
