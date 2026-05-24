import i18next from 'i18next'
import LanguageDetector from 'i18next-browser-languagedetector'
import { initReactI18next } from 'react-i18next'

import enCommon from '@/locales/en/common.json'
import enDashboard from '@/locales/en/dashboard.json'
import enDocker from '@/locales/en/docker.json'
import enFile from '@/locales/en/file.json'
import enFirewall from '@/locales/en/firewall.json'
import enIpQuality from '@/locales/en/ip-quality.json'
import enLogin from '@/locales/en/login.json'
import enNetwork from '@/locales/en/network.json'
import enOnboarding from '@/locales/en/onboarding.json'
import enSecurity from '@/locales/en/security.json'
import enServers from '@/locales/en/servers.json'
import enServiceMonitors from '@/locales/en/service-monitors.json'
import enSettings from '@/locales/en/settings.json'
import enStatus from '@/locales/en/status.json'
import enTerminal from '@/locales/en/terminal.json'

import zhCommon from '@/locales/zh/common.json'
import zhDashboard from '@/locales/zh/dashboard.json'
import zhDocker from '@/locales/zh/docker.json'
import zhFile from '@/locales/zh/file.json'
import zhFirewall from '@/locales/zh/firewall.json'
import zhIpQuality from '@/locales/zh/ip-quality.json'
import zhLogin from '@/locales/zh/login.json'
import zhNetwork from '@/locales/zh/network.json'
import zhOnboarding from '@/locales/zh/onboarding.json'
import zhSecurity from '@/locales/zh/security.json'
import zhServers from '@/locales/zh/servers.json'
import zhServiceMonitors from '@/locales/zh/service-monitors.json'
import zhSettings from '@/locales/zh/settings.json'
import zhStatus from '@/locales/zh/status.json'
import zhTerminal from '@/locales/zh/terminal.json'

i18next
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      en: {
        common: enCommon,
        dashboard: enDashboard,
        docker: enDocker,
        file: enFile,
        firewall: enFirewall,
        'ip-quality': enIpQuality,
        servers: enServers,
        'service-monitors': enServiceMonitors,
        terminal: enTerminal,
        settings: enSettings,
        login: enLogin,
        status: enStatus,
        network: enNetwork,
        onboarding: enOnboarding,
        security: enSecurity
      },
      zh: {
        common: zhCommon,
        dashboard: zhDashboard,
        docker: zhDocker,
        file: zhFile,
        firewall: zhFirewall,
        'ip-quality': zhIpQuality,
        servers: zhServers,
        'service-monitors': zhServiceMonitors,
        terminal: zhTerminal,
        settings: zhSettings,
        login: zhLogin,
        status: zhStatus,
        network: zhNetwork,
        onboarding: zhOnboarding,
        security: zhSecurity
      }
    },
    fallbackLng: 'en',
    defaultNS: 'common',
    detection: {
      order: ['localStorage', 'navigator']
    },
    interpolation: {
      escapeValue: false
    }
  })
