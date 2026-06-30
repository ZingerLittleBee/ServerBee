import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { Loader2, Upload } from 'lucide-react'
import { type ChangeEvent, type FormEvent, useEffect, useReducer, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button, buttonVariants } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { api } from '@/lib/api-client'

export const Route = createFileRoute('/_authed/settings/appearance')({
  component: AppearancePage
})

interface BrandSettings {
  favicon_url?: string
  footer_text?: string
  logo_url?: string
  site_title?: string
}

interface BrandFormState {
  faviconFile: File | null
  faviconPreview: string | null
  footerText: string
  logoFile: File | null
  logoPreview: string | null
  siteTitle: string
}

type BrandFormAction =
  | { type: 'brandLoaded'; brand: BrandSettings }
  | { type: 'faviconSelected'; file: File; preview: string }
  | { type: 'filesSaved' }
  | { type: 'footerTextChanged'; value: string }
  | { type: 'logoSelected'; file: File; preview: string }
  | { type: 'siteTitleChanged'; value: string }

const EMPTY_BRAND_FORM: BrandFormState = {
  faviconFile: null,
  faviconPreview: null,
  footerText: '',
  logoFile: null,
  logoPreview: null,
  siteTitle: ''
}

function brandFormReducer(state: BrandFormState, action: BrandFormAction): BrandFormState {
  switch (action.type) {
    case 'brandLoaded':
      return {
        ...state,
        faviconPreview: action.brand.favicon_url ?? null,
        footerText: action.brand.footer_text ?? '',
        logoPreview: action.brand.logo_url ?? null,
        siteTitle: action.brand.site_title ?? ''
      }
    case 'faviconSelected':
      return { ...state, faviconFile: action.file, faviconPreview: action.preview }
    case 'filesSaved':
      return { ...state, faviconFile: null, logoFile: null }
    case 'footerTextChanged':
      return { ...state, footerText: action.value }
    case 'logoSelected':
      return { ...state, logoFile: action.file, logoPreview: action.preview }
    case 'siteTitleChanged':
      return { ...state, siteTitle: action.value }
    default:
      return state
  }
}

function BrandSettingsSection() {
  const { t } = useTranslation(['settings', 'common'])
  const queryClient = useQueryClient()
  const logoInputRef = useRef<HTMLInputElement>(null)
  const faviconInputRef = useRef<HTMLInputElement>(null)
  const brandInitializedRef = useRef(false)

  const { data: brand } = useQuery<BrandSettings>({
    queryKey: ['settings', 'brand'],
    queryFn: () => api.get<BrandSettings>('/api/settings/brand')
  })

  const [form, dispatchForm] = useReducer(brandFormReducer, EMPTY_BRAND_FORM)

  useEffect(() => {
    if (!brand || brandInitializedRef.current) {
      return
    }
    brandInitializedRef.current = true
    dispatchForm({ type: 'brandLoaded', brand })
  }, [brand])

  const mutation = useMutation({
    mutationFn: async (payload: BrandSettings) => {
      if (form.logoFile || form.faviconFile) {
        const formData = new FormData()
        if (form.logoFile) {
          formData.append('logo', form.logoFile)
        }
        if (form.faviconFile) {
          formData.append('favicon', form.faviconFile)
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
      dispatchForm({ type: 'filesSaved' })
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
      const result = reader.result
      if (typeof result !== 'string') {
        return
      }
      if (type === 'logo') {
        dispatchForm({ type: 'logoSelected', file, preview: result })
      } else {
        dispatchForm({ type: 'faviconSelected', file, preview: result })
      }
    }
    reader.readAsDataURL(file)
  }

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    mutation.mutate({
      site_title: form.siteTitle,
      footer_text: form.footerText
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
            onChange={(e) => dispatchForm({ type: 'siteTitleChanged', value: e.target.value })}
            placeholder="ServerBee"
            value={form.siteTitle}
          />
        </div>

        <div className="space-y-1.5">
          <label className="font-medium text-sm" htmlFor="footer-text">
            {t('appearance.footer_text')}
          </label>
          <Input
            id="footer-text"
            onChange={(e) => dispatchForm({ type: 'footerTextChanged', value: e.target.value })}
            placeholder={t('appearance.footer_placeholder')}
            value={form.footerText}
          />
        </div>

        <div className="space-y-1.5">
          <label className="font-medium text-sm" htmlFor="logo-upload">
            {t('appearance.logo')}
          </label>
          <div className="flex items-center gap-3">
            {form.logoPreview && (
              <img
                alt="Logo preview"
                className="size-10 rounded-md border object-contain"
                height={40}
                src={form.logoPreview}
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
            {form.faviconPreview && (
              <img
                alt="Favicon preview"
                className="size-8 rounded border object-contain"
                height={32}
                src={form.faviconPreview}
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

function WidgetModulesNotice() {
  const { t } = useTranslation('settings')
  return (
    <div className="rounded-lg border bg-card p-6">
      <h2 className="mb-1 font-semibold text-lg">{t('appearance.theme_moved_title')}</h2>
      <p className="mb-4 text-muted-foreground text-sm">{t('appearance.theme_moved_description')}</p>
      <Link className={buttonVariants()} to="/settings/widgets">
        {t('appearance.theme_moved_cta')}
      </Link>
    </div>
  )
}

export function AppearancePage() {
  const { t } = useTranslation('settings')

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">{t('appearance.title')}</h1>
      <div className="max-w-3xl space-y-6">
        <WidgetModulesNotice />
        <BrandSettingsSection />
      </div>
    </div>
  )
}
