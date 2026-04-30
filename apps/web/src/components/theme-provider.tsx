/* eslint-disable react-refresh/only-export-components */
import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import { type ActiveThemeResponse, useActiveTheme, useSetActiveTheme } from '@/api/themes'
import { type ColorTheme, isColorTheme, loadThemeCSS } from '@/themes'

type Theme = 'dark' | 'light' | 'system'
type ResolvedTheme = 'dark' | 'light'

interface ThemeProviderProps {
  children: ReactNode
  colorThemeStorageKey?: string
  defaultTheme?: Theme
  disableTransitionOnChange?: boolean
  storageKey?: string
}

interface ThemeProviderState {
  activeTheme: ActiveThemeResponse | null
  colorTheme: ColorTheme
  setActiveThemeRef: (ref: string) => void
  setColorTheme: (colorTheme: ColorTheme) => void
  setTheme: (theme: Theme) => void
  theme: Theme
}

const COLOR_SCHEME_QUERY = '(prefers-color-scheme: dark)'
const ACTIVE_THEME_CACHE_KEY = 'active-theme-cache'
const THEME_RUNTIME_STYLE_ID = 'theme-runtime-style'
const THEME_VALUES: Theme[] = ['dark', 'light', 'system']

const ThemeProviderContext = createContext<ThemeProviderState | undefined>(undefined)

function isTheme(value: string | null): value is Theme {
  if (value === null) {
    return false
  }

  return THEME_VALUES.includes(value as Theme)
}

function isStringRecord(value: unknown): value is Record<string, string> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    return false
  }

  return Object.values(value).every((entry) => typeof entry === 'string')
}

function isActiveThemeResponse(value: unknown): value is ActiveThemeResponse {
  if (value === null || typeof value !== 'object') {
    return false
  }

  const candidate = value as Partial<ActiveThemeResponse>
  if (typeof candidate.ref !== 'string' || candidate.theme === null || typeof candidate.theme !== 'object') {
    return false
  }

  if (candidate.theme.kind === 'preset') {
    return typeof candidate.theme.id === 'string'
  }

  if (candidate.theme.kind === 'custom') {
    return (
      typeof candidate.theme.id === 'number' &&
      typeof candidate.theme.name === 'string' &&
      typeof candidate.theme.updated_at === 'string' &&
      isStringRecord(candidate.theme.vars_light) &&
      isStringRecord(candidate.theme.vars_dark)
    )
  }

  return false
}

function readActiveThemeCache(): ActiveThemeResponse | null {
  return parseActiveThemeCache(localStorage.getItem(ACTIVE_THEME_CACHE_KEY))
}

function parseActiveThemeCache(value: string | null): ActiveThemeResponse | null {
  if (value === null) {
    return null
  }

  try {
    const parsed: unknown = JSON.parse(value)
    if (isActiveThemeResponse(parsed)) {
      return parsed
    }
  } catch {
    return null
  }

  return null
}

function removeRuntimeStyle() {
  document.getElementById(THEME_RUNTIME_STYLE_ID)?.remove()
}

function cssVariableName(key: string) {
  return key.startsWith('--') ? key : `--${key}`
}

function serializeThemeVars(selector: string, vars: Record<string, string>) {
  const declarations = Object.entries(vars)
    .map(([key, value]) => `  ${cssVariableName(key)}: ${value};`)
    .join('\n')

  return `${selector} {\n${declarations}\n}`
}

function applyCustomTheme(activeTheme: ActiveThemeResponse) {
  if (activeTheme.theme.kind !== 'custom') {
    return
  }

  const style = document.createElement('style')
  style.id = THEME_RUNTIME_STYLE_ID
  style.textContent = [
    serializeThemeVars(':root', activeTheme.theme.vars_light),
    serializeThemeVars('.dark', activeTheme.theme.vars_dark)
  ].join('\n\n')
  document.head.appendChild(style)
}

function applyActiveThemeCacheStorageChange(
  newValue: string | null,
  setActiveThemeState: (theme: ActiveThemeResponse | null) => void
) {
  setActiveThemeState(parseActiveThemeCache(newValue))
}

function getSystemTheme(): ResolvedTheme {
  if (window.matchMedia(COLOR_SCHEME_QUERY).matches) {
    return 'dark'
  }

  return 'light'
}

function disableTransitionsTemporarily() {
  const style = document.createElement('style')
  style.appendChild(
    document.createTextNode('*,*::before,*::after{-webkit-transition:none!important;transition:none!important}')
  )
  document.head.appendChild(style)

  return () => {
    window.getComputedStyle(document.body)
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        style.remove()
      })
    })
  }
}

function isEditableTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) {
    return false
  }

  if (target.isContentEditable) {
    return true
  }

  const editableParent = target.closest("input, textarea, select, [contenteditable='true']")
  if (editableParent) {
    return true
  }

  return false
}

function getNextTheme(currentTheme: Theme): Theme {
  if (currentTheme === 'dark') {
    return 'light'
  }
  if (currentTheme === 'light') {
    return 'dark'
  }
  return getSystemTheme() === 'dark' ? 'light' : 'dark'
}

export function ThemeProvider({
  children,
  defaultTheme = 'system',
  storageKey = 'theme',
  colorThemeStorageKey: _colorThemeStorageKey = 'color-theme',
  disableTransitionOnChange = true,
  ...props
}: ThemeProviderProps) {
  const activeThemeQuery = useActiveTheme()
  const { mutate: setActiveTheme } = useSetActiveTheme()
  const [theme, setThemeState] = useState<Theme>(() => {
    const storedTheme = localStorage.getItem(storageKey)
    if (isTheme(storedTheme)) {
      return storedTheme
    }

    return defaultTheme
  })
  const [activeTheme, setActiveThemeState] = useState<ActiveThemeResponse | null>(() => readActiveThemeCache())

  const setTheme = useCallback(
    (nextTheme: Theme) => {
      localStorage.setItem(storageKey, nextTheme)
      setThemeState(nextTheme)
    },
    [storageKey]
  )

  const setActiveThemeRef = useCallback(
    (ref: string) => {
      setActiveTheme(ref)
    },
    [setActiveTheme]
  )

  const setColorTheme = useCallback(
    (nextColorTheme: ColorTheme) => {
      setActiveThemeRef(`preset:${nextColorTheme}`)
    },
    [setActiveThemeRef]
  )

  const applyTheme = useCallback(
    (nextTheme: Theme) => {
      const root = document.documentElement
      const resolvedTheme = nextTheme === 'system' ? getSystemTheme() : nextTheme
      const restoreTransitions = disableTransitionOnChange ? disableTransitionsTemporarily() : null

      root.classList.remove('light', 'dark')
      root.classList.add(resolvedTheme)

      if (restoreTransitions) {
        restoreTransitions()
      }
    },
    [disableTransitionOnChange]
  )

  useEffect(() => {
    if (activeThemeQuery.data === undefined) {
      return
    }

    setActiveThemeState(activeThemeQuery.data)
    localStorage.setItem(ACTIVE_THEME_CACHE_KEY, JSON.stringify(activeThemeQuery.data))
  }, [activeThemeQuery.data])

  // Apply light/dark theme
  useEffect(() => {
    applyTheme(theme)

    if (theme !== 'system') {
      return undefined
    }

    const mediaQuery = window.matchMedia(COLOR_SCHEME_QUERY)
    const handleChange = () => {
      applyTheme('system')
    }

    mediaQuery.addEventListener('change', handleChange)

    return () => {
      mediaQuery.removeEventListener('change', handleChange)
    }
  }, [theme, applyTheme])

  // Apply resolved color theme
  useEffect(() => {
    const root = document.documentElement
    let cancelled = false

    removeRuntimeStyle()

    if (activeTheme === null) {
      return undefined
    }

    if (activeTheme.theme.kind === 'custom') {
      root.removeAttribute('data-theme')
      applyCustomTheme(activeTheme)
      return undefined
    }

    const presetId = activeTheme.theme.id
    if (presetId === 'default') {
      root.removeAttribute('data-theme')
      return undefined
    }

    if (!isColorTheme(presetId)) {
      root.removeAttribute('data-theme')
      return undefined
    }

    loadThemeCSS(presetId).then(() => {
      if (cancelled) {
        return
      }

      root.setAttribute('data-theme', presetId)
    })

    return () => {
      cancelled = true
    }
  }, [activeTheme])

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.repeat) {
        return
      }

      if (event.metaKey || event.ctrlKey || event.altKey) {
        return
      }

      if (isEditableTarget(event.target)) {
        return
      }

      if (event.key.toLowerCase() !== 'd') {
        return
      }

      setThemeState((currentTheme) => {
        const nextTheme = getNextTheme(currentTheme)
        localStorage.setItem(storageKey, nextTheme)
        return nextTheme
      })
    }

    window.addEventListener('keydown', handleKeyDown)

    return () => {
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [storageKey])

  useEffect(() => {
    const handleStorageChange = (event: StorageEvent) => {
      if (event.storageArea !== localStorage) {
        return
      }

      if (event.key === storageKey) {
        if (isTheme(event.newValue)) {
          setThemeState(event.newValue)
          return
        }
        setThemeState(defaultTheme)
        return
      }

      if (event.key === ACTIVE_THEME_CACHE_KEY) {
        applyActiveThemeCacheStorageChange(event.newValue, setActiveThemeState)
      }
    }

    window.addEventListener('storage', handleStorageChange)

    return () => {
      window.removeEventListener('storage', handleStorageChange)
    }
  }, [defaultTheme, storageKey])

  const colorTheme = useMemo<ColorTheme>(() => {
    if (activeTheme?.theme.kind !== 'preset') {
      return 'default'
    }

    return isColorTheme(activeTheme.theme.id) ? activeTheme.theme.id : 'default'
  }, [activeTheme])

  const value = useMemo(
    () => ({
      theme,
      setTheme,
      activeTheme,
      colorTheme,
      setColorTheme,
      setActiveThemeRef
    }),
    [theme, setTheme, activeTheme, colorTheme, setColorTheme, setActiveThemeRef]
  )

  return (
    <ThemeProviderContext.Provider {...props} value={value}>
      {children}
    </ThemeProviderContext.Provider>
  )
}

export const useTheme = () => {
  const context = useContext(ThemeProviderContext)

  if (context === undefined) {
    throw new Error('useTheme must be used within a ThemeProvider')
  }

  return context
}
