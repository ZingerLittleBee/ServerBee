import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { Loader2, Upload } from 'lucide-react'
import { type ChangeEvent, type FormEvent, useRef, useState } from 'react'
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
