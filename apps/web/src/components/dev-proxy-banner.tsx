import { X } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { getDevProxyBannerState } from './dev-proxy-banner-state'

function getDevProxyTarget() {
  const target = import.meta.env.VITE_DEV_PROXY_TARGET

  if (typeof target === 'string' && target.length > 0) {
    return target
  }

  return 'unknown'
}

export function DevProxyBanner() {
  const { t } = useTranslation('common')
  const [dismissed, setDismissed] = useState(false)
  const state = getDevProxyBannerState({
    allowWrites: Reflect.get(import.meta.env, 'VITE_DEV_PROXY_ALLOW_WRITES'),
    mode: import.meta.env.MODE,
    target: getDevProxyTarget()
  })

  if (!state || dismissed) {
    return null
  }

  return (
    <div
      className={cn(state.className, 'pr-12')}
      role="alert"
      style={{ left: 0, pointerEvents: 'none', position: 'fixed', right: 0, top: 0, zIndex: 2_147_483_647 }}
    >
      {state.message}
      <Button
        aria-label={t('close')}
        className="pointer-events-auto absolute top-1/2 right-2 -translate-y-1/2 text-inherit hover:bg-black/10 hover:text-inherit"
        onClick={() => setDismissed(true)}
        size="icon-xs"
        type="button"
        variant="ghost"
      >
        <X aria-hidden="true" />
      </Button>
    </div>
  )
}
