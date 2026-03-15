import type enCommon from '@/locales/en/common.json'
import type enDashboard from '@/locales/en/dashboard.json'
import type enLogin from '@/locales/en/login.json'
import type enServers from '@/locales/en/servers.json'
import type enSettings from '@/locales/en/settings.json'
import type enStatus from '@/locales/en/status.json'
import type enTerminal from '@/locales/en/terminal.json'

declare module 'i18next' {
  interface CustomTypeOptions {
    defaultNS: 'common'
    resources: {
      common: typeof enCommon
      dashboard: typeof enDashboard
      servers: typeof enServers
      terminal: typeof enTerminal
      settings: typeof enSettings
      login: typeof enLogin
      status: typeof enStatus
    }
  }
}
