/* eslint-disable react-refresh/only-export-components */
import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useState } from 'react'
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
  colorTheme: ColorTheme
  setColorTheme: (colorTheme: ColorTheme) => void
  setTheme: (theme: Theme) => void
  theme: Theme
}

const COLOR_SCHEME_QUERY = '(prefers-color-scheme: dark)'
const THEME_VALUES: Theme[] = ['dark', 'light', 'system']

const ThemeProviderContext = createContext<ThemeProviderState | undefined>(undefined)

function isTheme(value: string | null): value is Theme {
  if (value === null) {
    return false
  }

  return THEME_VALUES.includes(value as Theme)
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
  colorThemeStorageKey = 'color-theme',
  disableTransitionOnChange = true,
  ...props
}: ThemeProviderProps) {
  const [theme, setThemeState] = useState<Theme>(() => {
    const storedTheme = localStorage.getItem(storageKey)
    if (isTheme(storedTheme)) {
      return storedTheme
    }

    return defaultTheme
  })

  const [colorTheme, setColorThemeState] = useState<ColorTheme>(() => {
    const stored = localStorage.getItem(colorThemeStorageKey)
    if (isColorTheme(stored)) {
      return stored
    }
    return 'default'
  })

  const setTheme = useCallback(
    (nextTheme: Theme) => {
      localStorage.setItem(storageKey, nextTheme)
      setThemeState(nextTheme)
    },
    [storageKey]
  )

  const setColorTheme = useCallback(
    (nextColorTheme: ColorTheme) => {
      localStorage.setItem(colorThemeStorageKey, nextColorTheme)
      setColorThemeState(nextColorTheme)
    },
    [colorThemeStorageKey]
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

  // Apply color theme
  useEffect(() => {
    const root = document.documentElement

    if (colorTheme === 'default') {
      root.removeAttribute('data-theme')
      return
    }

    loadThemeCSS(colorTheme).then(() => {
      root.setAttribute('data-theme', colorTheme)
    })
  }, [colorTheme])

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

      if (event.key === colorThemeStorageKey) {
        if (isColorTheme(event.newValue)) {
          setColorThemeState(event.newValue)
          return
        }
        setColorThemeState('default')
      }
    }

    window.addEventListener('storage', handleStorageChange)

    return () => {
      window.removeEventListener('storage', handleStorageChange)
    }
  }, [defaultTheme, storageKey, colorThemeStorageKey])

  const value = useMemo(
    () => ({
      theme,
      setTheme,
      colorTheme,
      setColorTheme
    }),
    [theme, setTheme, colorTheme, setColorTheme]
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
