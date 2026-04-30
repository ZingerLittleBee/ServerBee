import { useNavigate } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { type ExportPayload, type FullTheme, useThemeQuery, useUpdateTheme } from '@/api/themes'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { api } from '@/lib/api-client'
import { isColorTheme } from '@/themes'
import { type PresetVarKey, presetVars } from '@/themes/preset-vars'
import { OklchPicker } from './oklch-picker'
import { ThemePreview } from './theme-preview'

type ThemeMode = 'dark' | 'light'

const VAR_GROUPS: { id: string; vars: PresetVarKey[] }[] = [
  { id: 'surface', vars: ['background', 'foreground', 'card', 'card-foreground', 'popover', 'popover-foreground'] },
  { id: 'primary', vars: ['primary', 'primary-foreground', 'secondary', 'secondary-foreground'] },
  { id: 'state', vars: ['muted', 'muted-foreground', 'accent', 'accent-foreground', 'destructive'] },
  { id: 'border', vars: ['border', 'input', 'ring'] },
  { id: 'chart', vars: ['chart-1', 'chart-2', 'chart-3', 'chart-4', 'chart-5'] },
  {
    id: 'sidebar',
    vars: [
      'sidebar',
      'sidebar-foreground',
      'sidebar-primary',
      'sidebar-primary-foreground',
      'sidebar-accent',
      'sidebar-accent-foreground',
      'sidebar-border',
      'sidebar-ring'
    ]
  }
]

interface ThemeEditorProps {
  themeId: number
}

function isThemeMode(value: string | null): value is ThemeMode {
  return value === 'dark' || value === 'light'
}

function initialStateFromTheme(theme: FullTheme) {
  return {
    dark: theme.vars_dark,
    light: theme.vars_light,
    name: theme.name
  }
}

function downloadJsonFile(filename: string, payload: unknown) {
  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = filename
  anchor.click()
  URL.revokeObjectURL(url)
}

function safeFilename(name: string) {
  return (
    name
      .trim()
      .replaceAll(/[^a-zA-Z0-9._-]+/g, '-')
      .replaceAll(/^-|-$/g, '') || 'theme'
  )
}

function forkVarsForMode(theme: FullTheme, editingMode: ThemeMode) {
  if (!(theme.based_on && isColorTheme(theme.based_on))) {
    return null
  }

  if (editingMode === 'light') {
    return presetVars[theme.based_on].light
  }

  return presetVars[theme.based_on].dark
}

export function ThemeEditor({ themeId }: ThemeEditorProps) {
  const { t } = useTranslation(['settings', 'common'])
  const navigate = useNavigate()
  const { data, isLoading } = useThemeQuery(themeId)
  const updateTheme = useUpdateTheme()
  const [name, setName] = useState('')
  const [light, setLight] = useState<Record<string, string>>({})
  const [dark, setDark] = useState<Record<string, string>>({})
  const [editingMode, setEditingMode] = useState<ThemeMode>('light')
  const [previewMode, setPreviewMode] = useState<ThemeMode | null>(null)
  const [dirty, setDirty] = useState(false)
  const [exporting, setExporting] = useState(false)

  useEffect(() => {
    if (!data || dirty) {
      return
    }

    const initial = initialStateFromTheme(data)
    setName(initial.name)
    setLight(initial.light)
    setDark(initial.dark)
  }, [data, dirty])

  useEffect(() => {
    if (!dirty) {
      return undefined
    }

    const handler = (event: BeforeUnloadEvent) => {
      event.preventDefault()
      event.returnValue = ''
    }
    window.addEventListener('beforeunload', handler)

    return () => window.removeEventListener('beforeunload', handler)
  }, [dirty])

  if (isLoading || !data) {
    return <div className="p-6 text-muted-foreground text-sm">{t('common:loading')}</div>
  }

  const currentMap = editingMode === 'light' ? light : dark
  const forkVars = forkVarsForMode(data, editingMode)
  const previewWhich = previewMode ?? editingMode

  const setVar = (key: PresetVarKey, value: string) => {
    if (editingMode === 'light') {
      setLight((current) => ({ ...current, [key]: value }))
    } else {
      setDark((current) => ({ ...current, [key]: value }))
    }
    setDirty(true)
  }

  const resetVar = (key: PresetVarKey) => {
    const value = forkVars?.[key]
    if (!value) {
      return
    }
    setVar(key, value)
  }

  const save = () => {
    updateTheme.mutate(
      {
        body: {
          based_on: data.based_on,
          description: data.description,
          name,
          vars_dark: dark,
          vars_light: light
        },
        id: themeId
      },
      {
        onError: (error) => {
          toast.error(error instanceof Error ? error.message : t('common:errors.operation_failed'))
        },
        onSuccess: () => {
          toast.success(t('appearance.editor.saved'))
          setDirty(false)
          navigate({ to: '/settings/appearance' })
        }
      }
    )
  }

  const exportTheme = async () => {
    setExporting(true)
    try {
      const payload = await api.get<ExportPayload>(`/api/settings/themes/${themeId}/export`)
      downloadJsonFile(`${safeFilename(name)}.theme.json`, payload)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t('common:errors.operation_failed'))
    } finally {
      setExporting(false)
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-3 border-b p-4">
        <Input
          className="max-w-xs"
          onChange={(event) => {
            setName(event.target.value)
            setDirty(true)
          }}
          value={name}
        />
        {data.based_on && (
          <span className="text-muted-foreground text-sm">
            {t('appearance.editor.based_on', { name: data.based_on })}
          </span>
        )}
        <div className="ml-auto flex gap-2">
          <Button disabled={exporting} onClick={exportTheme} type="button" variant="outline">
            {t('appearance.custom_themes.export')}
          </Button>
          <Button onClick={() => navigate({ to: '/settings/appearance' })} type="button" variant="outline">
            {t('common:cancel')}
          </Button>
          <Button disabled={!dirty || updateTheme.isPending} onClick={save} type="button">
            {t('common:save')}
          </Button>
        </div>
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-2">
        <div className="min-h-0 border-r">
          <ScrollArea className="h-full">
            <div className="p-3">
              <Tabs
                onValueChange={(value) => {
                  if (isThemeMode(value)) {
                    setEditingMode(value)
                  }
                }}
                value={editingMode}
              >
                <TabsList className="mb-3">
                  <TabsTrigger value="light">{t('appearance.editor.light')}</TabsTrigger>
                  <TabsTrigger value="dark">{t('appearance.editor.dark')}</TabsTrigger>
                </TabsList>
              </Tabs>

              <div className="space-y-4">
                {VAR_GROUPS.map((group) => (
                  <section className="rounded-lg border p-3" key={group.id}>
                    <h2 className="mb-3 font-medium text-sm">{t(`appearance.editor.groups.${group.id}`)}</h2>
                    <div className="space-y-2">
                      {group.vars.map((key) => (
                        <div className="grid gap-2 sm:grid-cols-[8rem_1fr_auto] sm:items-center" key={key}>
                          <span className="font-mono text-xs">{key}</span>
                          <OklchPicker onChange={(value) => setVar(key, value)} value={currentMap[key] ?? ''} />
                          {forkVars && (
                            <Button onClick={() => resetVar(key)} size="sm" type="button" variant="ghost">
                              {t('appearance.editor.reset')}
                            </Button>
                          )}
                        </div>
                      ))}
                    </div>
                  </section>
                ))}
              </div>
            </div>
          </ScrollArea>
        </div>

        <div className="flex min-h-0 flex-col">
          <ThemePreview dark={previewWhich === 'dark'} vars={previewWhich === 'light' ? light : dark} />
          <div className="flex flex-wrap items-center gap-2 border-t p-3 text-xs">
            <span>{t('appearance.editor.preview_mode')}</span>
            <Button
              onClick={() => setPreviewMode(null)}
              size="sm"
              type="button"
              variant={previewMode === null ? 'default' : 'outline'}
            >
              {t('appearance.editor.linked')}
            </Button>
            <Button
              onClick={() => setPreviewMode('light')}
              size="sm"
              type="button"
              variant={previewMode === 'light' ? 'default' : 'outline'}
            >
              {t('appearance.editor.light')}
            </Button>
            <Button
              onClick={() => setPreviewMode('dark')}
              size="sm"
              type="button"
              variant={previewMode === 'dark' ? 'default' : 'outline'}
            >
              {t('appearance.editor.dark')}
            </Button>
          </div>
        </div>
      </div>
    </div>
  )
}
