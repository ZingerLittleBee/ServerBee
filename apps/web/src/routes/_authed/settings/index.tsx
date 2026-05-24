import { createFileRoute } from '@tanstack/react-router'
import { Trans, useTranslation } from 'react-i18next'
import { VersionRow } from '@/components/settings/about-card'
import { AsnRow } from '@/components/settings/asn-card'
import { GeoIpRow } from '@/components/settings/geoip-card'
import { SettingsSection } from '@/components/settings/settings-row'

export const Route = createFileRoute('/_authed/settings/')({
  component: SettingsPage
})

function SettingsPage() {
  const { t } = useTranslation('settings')

  return (
    <div className="w-full min-w-0 max-w-[calc(100vw-1.5rem)] overflow-hidden sm:max-w-full">
      <h1 className="mb-6 font-bold text-2xl">{t('title')}</h1>

      <div className="w-full min-w-0 max-w-3xl space-y-8">
        <SettingsSection
          footer={
            <Trans i18nKey="data_sources_attribution" t={t}>
              Data provided by{' '}
              <a className="underline" href="https://db-ip.com" rel="noopener noreferrer" target="_blank">
                DB-IP
              </a>
              , licensed under{' '}
              <a
                className="underline"
                href="https://creativecommons.org/licenses/by/4.0/"
                rel="noopener noreferrer"
                target="_blank"
              >
                CC BY 4.0
              </a>
              .
            </Trans>
          }
          title={t('section_data_sources')}
        >
          <GeoIpRow />
          <AsnRow />
        </SettingsSection>

        <SettingsSection title={t('section_about')}>
          <VersionRow />
        </SettingsSection>
      </div>
    </div>
  )
}
