import type { LayoutItem } from 'react-grid-layout'
import type { SizingStrategy } from '@/lib/widget-types'
import { COLS, MARGIN_X, MARGIN_Y, ROW_HEIGHT, SCALE } from '../grid-constants'

export interface NormalizeContext {
  autoMeasuredFineH?: number // populated for content-height widgets via ResizeObserver
  containerWidth: number
}

// Same formula RGL's aspectRatio(1) uses internally (see
// react-grid-layout@2.2.2 internals). Replicated here so we can
// apply it at idle render time — RGL constraints only run during resize.
export function pixelSquareFineH(wCoarse: number, containerWidth: number): number {
  if (containerWidth <= 0) {
    return wCoarse * SCALE
  }
  const colStepPx = (containerWidth + MARGIN_X) / COLS
  const fineRowStepPx = ROW_HEIGHT + MARGIN_Y
  return Math.max(1, Math.round((wCoarse * colStepPx) / fineRowStepPx))
}

// Applies strategy-specific overrides to a fine-unit layout item.
// Operates entirely in fine units (caller has already done `h *= SCALE`).
// Returns a NEW item — never mutates the input.
export function normalizeRenderItem(item: LayoutItem, strategy: SizingStrategy, ctx: NormalizeContext): LayoutItem {
  switch (strategy.kind) {
    case 'aspect-square': {
      const h = pixelSquareFineH(item.w, ctx.containerWidth)
      const minH = pixelSquareFineH(item.minW ?? 2, ctx.containerWidth)
      const maxH = item.maxW !== undefined ? pixelSquareFineH(item.maxW, ctx.containerWidth) : undefined
      return { ...item, h, minH, maxH }
    }
    case 'content-height': {
      const measured = ctx.autoMeasuredFineH
      if (measured !== undefined) {
        return { ...item, h: measured, minH: measured, maxH: measured }
      }
      return item
    }
    case 'fixed':
    case 'free':
      return item
  }
}
