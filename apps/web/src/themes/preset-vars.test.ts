import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, test } from 'vitest'
import type { ColorTheme } from './index'
import { PRESET_VAR_KEYS, presetVars } from './preset-vars'

const THEME_IDS: ColorTheme[] = [
  'default',
  'tokyo-night',
  'nord',
  'catppuccin',
  'dracula',
  'one-dark',
  'solarized',
  'rose-pine'
]

const THEME_FILE_BY_ID: Record<ColorTheme, string> = {
  default: join(process.cwd(), 'src/index.css'),
  'tokyo-night': join(process.cwd(), 'src/themes/tokyo-night.css'),
  nord: join(process.cwd(), 'src/themes/nord.css'),
  catppuccin: join(process.cwd(), 'src/themes/catppuccin.css'),
  dracula: join(process.cwd(), 'src/themes/dracula.css'),
  'one-dark': join(process.cwd(), 'src/themes/one-dark.css'),
  solarized: join(process.cwd(), 'src/themes/solarized.css'),
  'rose-pine': join(process.cwd(), 'src/themes/rose-pine.css')
}

function escapeRegex(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function readVars(selector: string, cssFile: string): Record<string, string> {
  const css = readFileSync(cssFile, 'utf8')
  const blockMatch = css.match(new RegExp(`${escapeRegex(selector)}\\s*\\{(?<body>[\\s\\S]*?)\\}`))

  if (!blockMatch?.groups?.body) {
    throw new Error(`Missing CSS block for ${selector}`)
  }

  return Object.fromEntries(
    Array.from(blockMatch.groups.body.matchAll(/--(?<key>[\w-]+):\s*(?<value>[^;]+);/g)).map((match) => [
      match.groups?.key ?? '',
      match.groups?.value.trim() ?? ''
    ])
  )
}

describe('presetVars', () => {
  test.each(THEME_IDS)('matches CSS variables for %s', (themeId) => {
    const lightSelector = themeId === 'default' ? ':root' : `[data-theme="${themeId}"]`
    const darkSelector = themeId === 'default' ? '.dark' : `[data-theme="${themeId}"].dark`
    const cssFile = THEME_FILE_BY_ID[themeId]

    const lightVars = readVars(lightSelector, cssFile)
    const darkVars = readVars(darkSelector, cssFile)

    for (const key of PRESET_VAR_KEYS) {
      expect(presetVars[themeId].light[key], `${themeId} light ${key}`).toBe(lightVars[key])
      expect(presetVars[themeId].dark[key], `${themeId} dark ${key}`).toBe(darkVars[key])
    }
  })
})
