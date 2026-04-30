import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Loader2, Plus, Upload } from 'lucide-react'
import { type ChangeEvent, type FormEvent, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { type ExportPayload, useCustomThemes, useDuplicateTheme, useImportTheme, useThemeQuery } from '@/api/themes'
import { DeleteThemeDialog } from '@/components/theme/delete-theme-dialog'
import { ThemeCard } from '@/components/theme/theme-card'
import { useTheme } from '@/components/theme-provider'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useAuth } from '@/hooks/use-auth'
import { api } from '@/lib/api-client'
import { oklchToHex } from '@/lib/oklch'
import { isColorTheme, themes } from '@/themes'

export const Route = createFileRoute('/_authed/settings/appearance')({
  component: AppearancePage
})

interface BrandSettings {
  favicon_url?: string
  footer_text?: string
  logo_url?: string
  site_title?: string
}

function useResolvedIsDark(theme: 'dark' | 'light' | 'system') {
  const [systemDark, setSystemDark] = useState(() => window.matchMedia('(prefers-color-scheme: dark)').matches)

  useEffect(() => {
    if (theme !== 'system') {
      return undefined
    }

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    const handleChange = () => setSystemDark(mediaQuery.matches)
    handleChange()
    mediaQuery.addEventListener('change', handleChange)

    return () => mediaQuery.removeEventListener('change', handleChange)
  }, [theme])

  return theme === 'dark' || (theme === 'system' && systemDark)
}

function useThemePreviewColors(id: number, dark: boolean) {
  const { data } = useThemeQuery(id)
  if (!data) {
    return []
  }

  const vars = dark ? data.vars_dark : data.vars_light
  return ['primary', 'accent', 'background', 'foreground'].map((key) => {
    const value = vars[key]
    if (!value) {
      return 'transparent'
    }

    return oklchToHex(value) ?? value
  })
}

function isStringMap(value: unknown): value is Record<string, string> {
  return (
    value !== null &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    Object.values(value).every((entry) => typeof entry === 'string')
  )
}

function hasProperty<Key extends string>(value: object, key: Key): value is Record<Key, unknown> {
  return key in value
}

function isThemeImportPayload(value: unknown): value is ExportPayload {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    return false
  }

  if (
    !(
      hasProperty(value, 'version') &&
      hasProperty(value, 'name') &&
      hasProperty(value, 'vars_light') &&
      hasProperty(value, 'vars_dark')
    )
  ) {
    return false
  }

  return (
    value.version === 1 &&
    typeof value.name === 'string' &&
    isStringMap(value.vars_light) &&
    isStringMap(value.vars_dark)
  )
}

function CustomThemeCard({
  active,
  id,
  name,
  onActivate,
  onDelete,
  onDuplicate,
  onEdit,
  previewDark
}: {
  active: boolean
  id: number
  name: string
  onActivate: () => void
  onDelete: () => void
  onDuplicate: () => void
  onEdit: () => void
  previewDark: boolean
}) {
  const preview = useThemePreviewColors(id, previewDark)

  return (
    <ThemeCard
      actions={{ onDelete, onDuplicate, onEdit }}
      active={active}
      name={name}
      onActivate={onActivate}
      preview={preview}
    />
  )
}

export function ThemeGrid() {
  const { t } = useTranslation('settings')
  const navigate = Route.useNavigate()
  const { activeTheme, setActiveThemeRef, theme } = useTheme()
  const isDark = useResolvedIsDark(theme)
  const { data: customThemes } = useCustomThemes()
  const duplicateTheme = useDuplicateTheme()
  const importTheme = useImportTheme()
  const fileInputRef = useRef<HTMLInputElement>(null)
  const [pendingDelete, setPendingDelete] = useState<{ id: number; name: string } | null>(null)
  const isActive = (themeRef: string) => activeTheme?.ref === themeRef
  const onImportFileChange = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    event.target.value = ''
    if (!file) {
      return
    }

    try {
      const parsed: unknown = JSON.parse(await file.text())
      if (!isThemeImportPayload(parsed)) {
        toast.error(t('appearance.custom_themes.import_invalid_json'))
        return
      }

      importTheme.mutate(parsed, {
        onError: (error) => {
          toast.error(error instanceof Error ? error.message : t('appearance.custom_themes.import_failed'))
        },
        onSuccess: () => {
          toast.success(t('appearance.custom_themes.imported'))
        }
      })
    } catch {
      toast.error(t('appearance.custom_themes.import_invalid_json'))
    }
  }

  return (
    <>
      <div className="rounded-lg border bg-card p-6">
        <h2 className="mb-1 font-semibold text-lg">{t('appearance.color_theme')}</h2>
        <p className="mb-4 text-muted-foreground text-sm">{t('appearance.color_theme_description')}</p>

        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
          {themes.map((themeInfo) => (
            <ThemeCard
              active={isActive(`preset:${themeInfo.id}`)}
              key={themeInfo.id}
              name={themeInfo.name}
              onActivate={() => setActiveThemeRef(`preset:${themeInfo.id}`)}
              preview={isDark ? themeInfo.previewColors.dark : themeInfo.previewColors.light}
            />
          ))}
        </div>
      </div>

      <div className="rounded-lg border bg-card p-6">
        <div className="mb-4 flex items-center justify-between gap-3">
          <div>
            <h2 className="font-semibold text-lg">{t('appearance.custom_themes.title')}</h2>
            <p className="text-muted-foreground text-sm">{t('appearance.custom_themes.description')}</p>
          </div>
          <div className="flex gap-2">
            <input
              accept="application/json"
              className="hidden"
              onChange={onImportFileChange}
              ref={fileInputRef}
              type="file"
            />
            <Button onClick={() => fileInputRef.current?.click()} type="button" variant="outline">
              <Upload className="size-4" />
              {t('appearance.custom_themes.import')}
            </Button>
            <Button onClick={() => navigate({ to: '/settings/appearance/themes/new' })} type="button">
              <Plus className="size-4" />
              {t('appearance.custom_themes.new')}
            </Button>
          </div>
        </div>

        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
          {(customThemes ?? []).map((customTheme) => (
            <CustomThemeCard
              active={isActive(`custom:${customTheme.id}`)}
              id={customTheme.id}
              key={customTheme.id}
              name={customTheme.name}
              onActivate={() => setActiveThemeRef(`custom:${customTheme.id}`)}
              onDelete={() => setPendingDelete({ id: customTheme.id, name: customTheme.name })}
              onDuplicate={() => duplicateTheme.mutate(customTheme.id)}
              onEdit={() =>
                navigate({
                  params: { id: String(customTheme.id) },
                  to: '/settings/appearance/themes/$id'
                })
              }
              previewDark={isDark}
            />
          ))}
        </div>
      </div>

      {pendingDelete && <DeleteThemeDialog onClose={() => setPendingDelete(null)} theme={pendingDelete} />}
    </>
  )
}

export function LegacyMigrationPrompt() {
  const { t } = useTranslation('settings')
  const { user } = useAuth()
  const { activeTheme, setActiveThemeRef } = useTheme()
  const [dismissed, setDismissed] = useState(() => localStorage.getItem('theme-migration-prompted') === '1')

  if (dismissed || user?.role !== 'admin') {
    return null
  }

  const legacy = localStorage.getItem('color-theme')
  if (!(legacy && isColorTheme(legacy)) || activeTheme?.ref !== 'preset:default') {
    return null
  }

  const dismiss = () => {
    localStorage.setItem('theme-migration-prompted', '1')
    localStorage.removeItem('color-theme')
    setDismissed(true)
  }

  const apply = () => {
    setActiveThemeRef(`preset:${legacy}`)
    dismiss()
  }

  return (
    <div className="mb-4 flex items-center justify-between gap-3 rounded-lg border bg-muted p-4">
      <span className="text-sm">{t('appearance.legacy_theme_migration.detected', { theme: legacy })}</span>
      <div className="flex gap-2">
        <Button onClick={dismiss} size="sm" type="button" variant="outline">
          {t('appearance.legacy_theme_migration.ignore')}
        </Button>
        <Button onClick={apply} size="sm" type="button">
          {t('appearance.legacy_theme_migration.apply')}
        </Button>
      </div>
    </div>
  )
}

function BrandSettingsSection() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const logoInputRef = useRef<HTMLInputElement>(null)
  const faviconInputRef = useRef<HTMLInputElement>(null)

  const { data: brand } = useQuery<BrandSettings>({
    queryKey: ['settings', 'brand'],
    queryFn: () => api.get<BrandSettings>('/api/settings/brand')
  })

  const [siteTitle, setSiteTitle] = useState('')
  const [footerText, setFooterText] = useState('')
  const [logoPreview, setLogoPreview] = useState<string | null>(null)
  const [faviconPreview, setFaviconPreview] = useState<string | null>(null)
  const [logoFile, setLogoFile] = useState<File | null>(null)
  const [faviconFile, setFaviconFile] = useState<File | null>(null)
  const [initialized, setInitialized] = useState(false)

  if (brand && !initialized) {
    setSiteTitle(brand.site_title ?? '')
    setFooterText(brand.footer_text ?? '')
    if (brand.logo_url) {
      setLogoPreview(brand.logo_url)
    }
    if (brand.favicon_url) {
      setFaviconPreview(brand.favicon_url)
    }
    setInitialized(true)
  }

  const mutation = useMutation({
    mutationFn: async (payload: BrandSettings) => {
      // If files are selected, upload them first via FormData
      if (logoFile || faviconFile) {
        const formData = new FormData()
        if (logoFile) {
          formData.append('logo', logoFile)
        }
        if (faviconFile) {
          formData.append('favicon', faviconFile)
        }
        formData.append('site_title', payload.site_title ?? '')
        formData.append('footer_text', payload.footer_text ?? '')

        const response = await fetch('/api/settings/brand', {
          method: 'PUT',
          credentials: 'include',
          body: formData
        })
        if (!response.ok) {
          const text = await response.text().catch(() => response.statusText)
          throw new Error(text)
        }
        return
      }

      return api.put('/api/settings/brand', payload)
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['settings', 'brand'] }).catch(() => undefined)
      setLogoFile(null)
      setFaviconFile(null)
      toast.success(t('appearance.brand_saved'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('common:errors.operation_failed'))
    }
  })

  const handleFileChange = (e: ChangeEvent<HTMLInputElement>, type: 'favicon' | 'logo') => {
    const file = e.target.files?.[0]
    if (!file) {
      return
    }

    const reader = new FileReader()
    reader.onloadend = () => {
      const result = reader.result as string
      if (type === 'logo') {
        setLogoPreview(result)
        setLogoFile(file)
      } else {
        setFaviconPreview(result)
        setFaviconFile(file)
      }
    }
    reader.readAsDataURL(file)
  }

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    mutation.mutate({
      site_title: siteTitle,
      footer_text: footerText
    })
  }

  return (
    <div className="rounded-lg border bg-card p-6">
      <h2 className="mb-1 font-semibold text-lg">{t('appearance.brand_settings')}</h2>
      <p className="mb-4 text-muted-foreground text-sm">{t('appearance.brand_description')}</p>

      <form className="max-w-lg space-y-4" onSubmit={handleSubmit}>
        <div className="space-y-1.5">
          <label className="font-medium text-sm" htmlFor="site-title">
            {t('appearance.site_title')}
          </label>
          <Input
            id="site-title"
            onChange={(e) => setSiteTitle(e.target.value)}
            placeholder="ServerBee"
            value={siteTitle}
          />
        </div>

        <div className="space-y-1.5">
          <label className="font-medium text-sm" htmlFor="footer-text">
            {t('appearance.footer_text')}
          </label>
          <Input
            id="footer-text"
            onChange={(e) => setFooterText(e.target.value)}
            placeholder={t('appearance.footer_placeholder')}
            value={footerText}
          />
        </div>

        <div className="space-y-1.5">
          <label className="font-medium text-sm" htmlFor="logo-upload">
            {t('appearance.logo')}
          </label>
          <div className="flex items-center gap-3">
            {logoPreview && (
              <img
                alt="Logo preview"
                className="size-10 rounded-md border object-contain"
                height={40}
                src={logoPreview}
                width={40}
              />
            )}
            <Button onClick={() => logoInputRef.current?.click()} size="sm" type="button" variant="outline">
              <Upload className="size-3.5" />
              {t('appearance.upload_logo')}
            </Button>
            <input
              accept="image/*"
              className="hidden"
              id="logo-upload"
              onChange={(e) => handleFileChange(e, 'logo')}
              ref={logoInputRef}
              type="file"
            />
          </div>
        </div>

        <div className="space-y-1.5">
          <label className="font-medium text-sm" htmlFor="favicon-upload">
            {t('appearance.favicon')}
          </label>
          <div className="flex items-center gap-3">
            {faviconPreview && (
              <img
                alt="Favicon preview"
                className="size-8 rounded border object-contain"
                height={32}
                src={faviconPreview}
                width={32}
              />
            )}
            <Button onClick={() => faviconInputRef.current?.click()} size="sm" type="button" variant="outline">
              <Upload className="size-3.5" />
              {t('appearance.upload_favicon')}
            </Button>
            <input
              accept="image/*"
              className="hidden"
              id="favicon-upload"
              onChange={(e) => handleFileChange(e, 'favicon')}
              ref={faviconInputRef}
              type="file"
            />
          </div>
        </div>

        {mutation.error && (
          <p className="text-destructive text-sm">{mutation.error.message || t('appearance.save_failed')}</p>
        )}

        <Button disabled={mutation.isPending} type="submit">
          {mutation.isPending ? <Loader2 className="size-4 animate-spin" /> : null}
          {mutation.isPending ? t('common:saving') : t('common:save')}
        </Button>
      </form>
    </div>
  )
}

function AppearancePage() {
  const { t } = useTranslation('settings')

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">{t('appearance.title')}</h1>
      <LegacyMigrationPrompt />
      <div className="max-w-3xl space-y-6">
        <ThemeGrid />
        <BrandSettingsSection />
      </div>
    </div>
  )
}
