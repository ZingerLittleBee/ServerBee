import { createFileRoute } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { AboutCard } from '@/components/settings/about-card'
import { AsnCard } from '@/components/settings/asn-card'
import { GeoIpCard } from '@/components/settings/geoip-card'

export const Route = createFileRoute('/_authed/settings/')({
  component: SettingsPage
})

function SettingsPage() {
  const { t } = useTranslation('settings')

  return (
    <div className="w-full min-w-0 max-w-[calc(100vw-1.5rem)] overflow-hidden sm:max-w-full">
      <h1 className="mb-6 font-bold text-2xl">{t('title')}</h1>

      <div className="w-full min-w-0 max-w-xl space-y-6">
        <GeoIpCard />
        <AsnCard />
        <AboutCard />
      </div>
    </div>
  )
}
