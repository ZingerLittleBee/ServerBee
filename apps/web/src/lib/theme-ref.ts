import { type ColorTheme, isColorTheme } from '@/themes'

export type ThemeRef = { kind: 'preset'; id: ColorTheme } | { kind: 'custom'; id: number }

const CUSTOM_ID_PATTERN = /^[1-9]\d*$/
const MAX_CUSTOM_ID = 2_147_483_647

export function parseThemeRef(s: string): ThemeRef | null {
  if (s.startsWith('preset:')) {
    const id = s.slice('preset:'.length)
    return isColorTheme(id) ? { kind: 'preset', id } : null
  }

  if (s.startsWith('custom:')) {
    const id = parseCustomId(s.slice('custom:'.length))
    return id === null ? null : { kind: 'custom', id }
  }

  return null
}

export function themeRefToString(r: ThemeRef): string {
  return r.kind === 'preset' ? `preset:${r.id}` : `custom:${r.id}`
}

function parseCustomId(raw: string): number | null {
  if (!CUSTOM_ID_PATTERN.test(raw)) {
    return null
  }

  const id = Number(raw)
  return id <= MAX_CUSTOM_ID ? id : null
}
