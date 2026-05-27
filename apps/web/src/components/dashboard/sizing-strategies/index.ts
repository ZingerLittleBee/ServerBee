import type { LayoutItem } from 'react-grid-layout'
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
