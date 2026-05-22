// Single source of truth for the IP-quality service category set.
//
// The category keys mirror `unlock_service.category` on the server. Adding a
// new category here keeps the matrix grouping, the settings catalog, and the
// custom-service dialog in sync — they all derive from this module.

/** Canonical category keys, in display order. */
export const CATEGORY_ORDER = ['streaming', 'ai', 'social', 'gaming', 'other'] as const

export type ServiceCategory = (typeof CATEGORY_ORDER)[number]

const CATEGORY_LABELS: Record<string, string> = {
  streaming: 'Streaming',
  ai: 'AI',
  social: 'Social',
  gaming: 'Gaming',
  other: 'Other'
}

/** Human-readable label for a category, falling back to the raw key. */
export function categoryLabel(category: string): string {
  return CATEGORY_LABELS[category] ?? category
}

/** Sort rank for a category; unknown categories sort after the known ones. */
export function categoryRank(category: string): number {
  const idx = CATEGORY_ORDER.indexOf(category as ServiceCategory)
  return idx === -1 ? CATEGORY_ORDER.length : idx
}
