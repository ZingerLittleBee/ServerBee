import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { Copy, Eye, EyeOff } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api-client'
import type { AutoDiscoveryKeyResponse } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/')({
  component: SettingsPage
})

function SettingsPage() {
  const { t } = useTranslation('settings')
  const [showKey, setShowKey] = useState(false)
  const [copied, setCopied] = useState(false)

  const { data: config } = useQuery<AutoDiscoveryKeyResponse>({
    queryKey: ['settings', 'discovery'],
    queryFn: () => api.get<AutoDiscoveryKeyResponse>('/api/settings/auto-discovery-key')
  })

  const handleCopy = async () => {
    if (!config?.key) {
      return
    }
    try {
      await navigator.clipboard.writeText(config.key)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      // Clipboard access denied
    }
  }

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">{t('title')}</h1>

      <div className="max-w-xl space-y-6">
        <div className="rounded-lg border bg-card p-6">
          <h2 className="mb-1 font-semibold text-lg">{t('auto_discovery_key')}</h2>
          <p className="mb-4 text-muted-foreground text-sm">{t('auto_discovery_description')}</p>

          {config?.key ? (
            <div className="flex items-center gap-2">
              <div className="flex-1 rounded-md border bg-muted/50 px-3 py-2 font-mono text-sm">
                {showKey ? config.key : config.key.replace(/./g, '*')}
              </div>
              <Button
                aria-label={showKey ? t('hide_key') : t('show_key')}
                onClick={() => setShowKey((prev) => !prev)}
                size="icon"
                variant="outline"
              >
                {showKey ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
              </Button>
              <Button aria-label={t('copy_key')} onClick={handleCopy} size="icon" variant="outline">
                <Copy className="size-4" />
              </Button>
              {copied && <span className="text-emerald-600 text-xs dark:text-emerald-400">{t('copied')}</span>}
            </div>
          ) : (
            <div className="h-10 animate-pulse rounded-md bg-muted" />
          )}
        </div>
      </div>
    </div>
  )
}
