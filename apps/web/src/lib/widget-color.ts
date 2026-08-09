/** Shared default chart accent for configurable single-host widgets. */
export const DEFAULT_WIDGET_CHART_COLOR = '#8EC5FF'

/** Secondary series default (disk write / traffic out). */
export const DEFAULT_WIDGET_CHART_COLOR_SECONDARY = '#34D399'

const HEX6_RE = /^#[0-9A-Fa-f]{6}$/
const HEX3_RE = /^#[0-9A-Fa-f]{3}$/

/** Normalize user-entered hex (`#RGB`, `#RRGGBB`, optional leading `#`) to `#RRGGBB`. */
export function normalizeWidgetColor(value: string | undefined | null): string | null {
  if (value == null) {
    return null
  }
  const trimmed = value.trim()
  if (trimmed.length === 0) {
    return null
  }
  const withHash = trimmed.startsWith('#') ? trimmed : `#${trimmed}`
  if (HEX6_RE.test(withHash)) {
    return withHash.toUpperCase()
  }
  if (HEX3_RE.test(withHash)) {
    const r = withHash[1]
    const g = withHash[2]
    const b = withHash[3]
    return `#${r}${r}${g}${g}${b}${b}`.toUpperCase()
  }
  return null
}

export function resolveWidgetColor(
  value: string | undefined | null,
  fallback: string = DEFAULT_WIDGET_CHART_COLOR
): string {
  return normalizeWidgetColor(value) ?? fallback
}

/** Relative luminance of `#RRGGBB` (sRGB). */
export function hexRelativeLuminance(hex: string): number {
  const raw = hex.replace('#', '')
  const channels = [0, 2, 4].map((offset) => {
    const channel = Number.parseInt(raw.slice(offset, offset + 2), 16) / 255
    return channel <= 0.039_28 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4
  })
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2]
}

/** Swatches shown in widget color pickers. */
export const WIDGET_COLOR_SWATCHES = [
  DEFAULT_WIDGET_CHART_COLOR,
  '#60A5FA',
  DEFAULT_WIDGET_CHART_COLOR_SECONDARY,
  '#FBBF24',
  '#F87171',
  '#A78BFA',
  '#F472B6',
  '#94A3B8'
] as const
