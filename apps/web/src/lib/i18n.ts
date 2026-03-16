import i18next from 'i18next'
import LanguageDetector from 'i18next-browser-languagedetector'
import { initReactI18next } from 'react-i18next'

import enCommon from '@/locales/en/common.json'
import enDashboard from '@/locales/en/dashboard.json'
import enFile from '@/locales/en/file.json'
import enLogin from '@/locales/en/login.json'
import enNetwork from '@/locales/en/network.json'
import enServers from '@/locales/en/servers.json'
import enSettings from '@/locales/en/settings.json'
import enStatus from '@/locales/en/status.json'
import enTerminal from '@/locales/en/terminal.json'

import zhCommon from '@/locales/zh/common.json'
import zhDashboard from '@/locales/zh/dashboard.json'
import zhFile from '@/locales/zh/file.json'
import zhLogin from '@/locales/zh/login.json'
import zhNetwork from '@/locales/zh/network.json'
import zhServers from '@/locales/zh/servers.json'
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
        file: enFile,
        servers: enServers,
        terminal: enTerminal,
        settings: enSettings,
        login: enLogin,
        status: enStatus,
        network: enNetwork
      },
      zh: {
        common: zhCommon,
        dashboard: zhDashboard,
        file: zhFile,
        servers: zhServers,
        terminal: zhTerminal,
        settings: zhSettings,
        login: zhLogin,
        status: zhStatus,
        network: zhNetwork
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
