import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { Loader2, Upload } from 'lucide-react'
import { type ChangeEvent, type FormEvent, useEffect, useReducer, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { PageBody } from '@/components/layout/page-body'
import { Button } from '@/components/ui/button'
import { buttonVariants } from '@/components/ui/button-variants'
import { Input } from '@/components/ui/input'
import { api } from '@/lib/api-client'

export const Route = createFileRoute('/_authed/settings/appearance')({
  component: AppearancePage
})

// Mirrors the server BrandConfig (crates/server/src/router/api/brand.rs): the
// API returns `logo_path`/`favicon_path` (URLs like `/api/brand/logo`), NOT
// `*_url`. Files are uploaded via dedicated multipart POST endpoints, then the
// paths are persisted with a JSON PUT.
interface BrandSettings {
  favicon_path?: string | null
  footer_text?: string | null
  logo_path?: string | null
  site_title?: string | null
}

interface BrandFormState {
  faviconFile: File | null
  faviconPath: string | null
  faviconPreview: string | null
  footerText: string
  logoFile: File | null
  logoPath: string | null
  logoPreview: string | null
  siteTitle: string
}

type BrandFormAction =
  | { type: 'brandLoaded'; brand: BrandSettings }
  | { type: 'faviconSelected'; file: File; preview: string }
  | { type: 'filesSaved'; faviconPath: string | null; logoPath: string | null }
  | { type: 'footerTextChanged'; value: string }
  | { type: 'logoSelected'; file: File; preview: string }
  | { type: 'siteTitleChanged'; value: string }

const EMPTY_BRAND_FORM: BrandFormState = {
  faviconFile: null,
  faviconPath: null,
  faviconPreview: null,
  footerText: '',
  logoFile: null,
  logoPath: null,
  logoPreview: null,
  siteTitle: ''
}

function brandFormReducer(state: BrandFormState, action: BrandFormAction): BrandFormState {
  switch (action.type) {
    case 'brandLoaded':
      return {
        ...state,
        faviconPath: action.brand.favicon_path ?? null,
        faviconPreview: action.brand.favicon_path ?? null,
        footerText: action.brand.footer_text ?? '',
        logoPath: action.brand.logo_path ?? null,
        logoPreview: action.brand.logo_path ?? null,
        siteTitle: action.brand.site_title ?? ''
      }
    case 'faviconSelected':
      return { ...state, faviconFile: action.file, faviconPreview: action.preview }
    case 'filesSaved':
      return {
        ...state,
        faviconFile: null,
        faviconPath: action.faviconPath,
        logoFile: null,
        logoPath: action.logoPath
      }
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

// Upload a brand image to its dedicated multipart endpoint (field name "file",
// PNG/ICO only — validated server-side) and return the served path.
async function uploadBrandImage(kind: 'favicon' | 'logo', file: File): Promise<string> {
  const formData = new FormData()
  formData.append('file', file)
  const response = await fetch(`/api/settings/brand/${kind}`, {
    method: 'POST',
    credentials: 'include',
    body: formData
  })
  if (!response.ok) {
    const text = await response.text().catch(() => response.statusText)
    let message = text
    try {
      message = JSON.parse(text)?.error?.message || text
    } catch {
      // body is not JSON; use the raw text
    }
    throw new Error(message)
  }
  const json = await response.json()
  return (json?.data?.path as string | undefined) ?? `/api/brand/${kind}`
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
    mutationFn: async () => {
      // Upload any newly selected images first (each endpoint persists its own
      // path server-side), then write the full config in one JSON PUT that
      // preserves the existing logo/favicon paths so a title-only edit no longer
      // wipes a previously uploaded logo.
      let logoPath = form.logoPath
      let faviconPath = form.faviconPath
      if (form.logoFile) {
        logoPath = await uploadBrandImage('logo', form.logoFile)
      }
      if (form.faviconFile) {
        faviconPath = await uploadBrandImage('favicon', form.faviconFile)
      }
      await api.put('/api/settings/brand', {
        site_title: form.siteTitle,
        footer_text: form.footerText,
        logo_path: logoPath,
        favicon_path: faviconPath
      })
      return { faviconPath, logoPath }
    },
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['settings', 'brand'] }).catch(() => undefined)
      dispatchForm({ type: 'filesSaved', faviconPath: result.faviconPath, logoPath: result.logoPath })
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
    mutation.mutate()
  }

  return (
    <section>
      <h2 className="mb-2 px-1 font-medium text-muted-foreground text-sm">{t('appearance.brand_settings')}</h2>
      <div className="space-y-4 px-1">
        <p className="text-muted-foreground text-sm">{t('appearance.brand_description')}</p>

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
                  alt={t('appearance.logo_preview')}
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
                accept=".png,.ico,image/png,image/x-icon"
                className="hidden"
                id="logo-upload"
                onChange={(e) => handleFileChange(e, 'logo')}
                ref={logoInputRef}
                type="file"
              />
            </div>
            <p className="text-muted-foreground text-xs">{t('appearance.image_hint')}</p>
          </div>

          <div className="space-y-1.5">
            <label className="font-medium text-sm" htmlFor="favicon-upload">
              {t('appearance.favicon')}
            </label>
            <div className="flex items-center gap-3">
              {form.faviconPreview && (
                <img
                  alt={t('appearance.favicon_preview')}
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
                accept=".png,.ico,image/png,image/x-icon"
                className="hidden"
                id="favicon-upload"
                onChange={(e) => handleFileChange(e, 'favicon')}
                ref={faviconInputRef}
                type="file"
              />
            </div>
            <p className="text-muted-foreground text-xs">{t('appearance.image_hint')}</p>
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
    </section>
  )
}

function WidgetModulesNotice() {
  const { t } = useTranslation('settings')
  return (
    <section>
      <h2 className="mb-2 px-1 font-medium text-muted-foreground text-sm">{t('appearance.theme_moved_title')}</h2>
      <div className="space-y-4 px-1">
        <p className="text-muted-foreground text-sm">{t('appearance.theme_moved_description')}</p>
        <Link className={buttonVariants()} to="/settings/widgets">
          {t('appearance.theme_moved_cta')}
        </Link>
      </div>
    </section>
  )
}

export function AppearancePage() {
  return (
    <PageBody>
      <div className="w-full min-w-0 max-w-[calc(100vw-1.5rem)] sm:max-w-full">
        <div className="w-full min-w-0 max-w-3xl space-y-8">
          <WidgetModulesNotice />
          <BrandSettingsSection />
        </div>
      </div>
    </PageBody>
  )
}
