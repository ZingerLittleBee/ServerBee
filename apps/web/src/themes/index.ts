export type ColorTheme =
  | 'default'
  | 'tokyo-night'
  | 'nord'
  | 'catppuccin'
  | 'dracula'
  | 'one-dark'
  | 'solarized'
  | 'rose-pine'

export interface ThemeInfo {
  id: ColorTheme
  name: string
  previewColors: { light: string[]; dark: string[] }
}

export const themes: ThemeInfo[] = [
  {
    id: 'default',
    name: 'Default',
    previewColors: {
      light: ['#000', '#fff', '#f5f5f5', '#e5e5e5'],
      dark: ['#fafafa', '#171717', '#262626', '#404040']
    }
  },
  {
    id: 'tokyo-night',
    name: 'Tokyo Night',
    previewColors: {
      light: ['#7a5af5', '#f0edf8', '#534080', '#c4b5f0'],
      dark: ['#bb9af7', '#1a1b2e', '#2a2b45', '#7aa2f7']
    }
  },
  {
    id: 'nord',
    name: 'Nord',
    previewColors: {
      light: ['#5e81ac', '#eceff4', '#4c566a', '#d8dee9'],
      dark: ['#88c0d0', '#2e3440', '#3b4252', '#81a1c1']
    }
  },
  {
    id: 'catppuccin',
    name: 'Catppuccin',
    previewColors: {
      light: ['#ca7cf8', '#f2ecf8', '#6c4a8a', '#e6d0f5'],
      dark: ['#cba6f7', '#1e1e2e', '#313244', '#f5c2e7']
    }
  },
  {
    id: 'dracula',
    name: 'Dracula',
    previewColors: {
      light: ['#7c3aed', '#f2eff8', '#503a80', '#c4b0f0'],
      dark: ['#bd93f9', '#282a36', '#44475a', '#ff79c6']
    }
  },
  {
    id: 'one-dark',
    name: 'One Dark',
    previewColors: {
      light: ['#4078f2', '#f0f2f5', '#383c44', '#c8ccd4'],
      dark: ['#61afef', '#282c34', '#3e4452', '#e5c07b']
    }
  },
  {
    id: 'solarized',
    name: 'Solarized',
    previewColors: {
      light: ['#268bd2', '#fdf6e3', '#586e75', '#eee8d5'],
      dark: ['#2aa198', '#002b36', '#073642', '#b58900']
    }
  },
  {
    id: 'rose-pine',
    name: 'Rose Pine',
    previewColors: {
      light: ['#d7827e', '#f4ede8', '#575279', '#dfdad9'],
      dark: ['#ebbcba', '#191724', '#26233a', '#f6c177']
    }
  }
]

const COLOR_THEME_VALUES: ColorTheme[] = [
  'default',
  'tokyo-night',
  'nord',
  'catppuccin',
  'dracula',
  'one-dark',
  'solarized',
  'rose-pine'
]

export function isColorTheme(value: string | null): value is ColorTheme {
  if (value === null) {
    return false
  }
  return COLOR_THEME_VALUES.includes(value as ColorTheme)
}

const loadedThemes = new Set<string>()

export async function loadThemeCSS(themeId: ColorTheme): Promise<void> {
  if (themeId === 'default' || loadedThemes.has(themeId)) {
    return
  }
  await import(`./${themeId}.css`)
  loadedThemes.add(themeId)
}
