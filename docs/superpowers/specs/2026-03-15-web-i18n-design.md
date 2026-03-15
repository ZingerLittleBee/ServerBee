# Web Frontend i18n Design

## Overview

Add internationalization (i18n) support to the ServerBee web frontend (`apps/web`), enabling Chinese + English bilingual UI with browser-based language detection and manual switching.

## Goals

- Full-site i18n coverage: all user-visible text across login, dashboard, servers, terminal, settings, and status pages (~150+ strings)
- Two languages: English (en) and Chinese (zh), matching the existing fumadocs documentation site
- Browser language auto-detection with manual toggle in the UI
- Language preference persisted in localStorage
- TypeScript type-safe translation keys

## Non-Goals

- Number/date/byte formatting localization (formats are language-agnostic in this context)
- Server-side language preference persistence (localStorage is sufficient)
- Translating dynamic data from API (server names, group names, etc.)
- Translating the brand name "ServerBee"
- Additional languages beyond en/zh in this iteration

## Technology Choice

**react-i18next** (i18next + react-i18next + i18next-browser-languagedetector)

Rationale:
- Community standard with mature ecosystem and extensive documentation
- Native namespace support matching the per-route file organization requirement
- Built-in browser language detection and localStorage persistence via plugins
- TypeScript type safety via module augmentation
- ~13KB gzipped total — acceptable for a monitoring dashboard
- Chosen over self-built (too manual) and lingui (extra build step, less community adoption)

## Architecture

### Dependencies

```
i18next
react-i18next
i18next-browser-languagedetector
```

### Initialization

File: `src/lib/i18n.ts`

```typescript
import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import LanguageDetector from 'i18next-browser-languagedetector'

// Static imports for all namespaces (150+ strings, no need for lazy loading)
import enCommon from '@/locales/en/common.json'
import enDashboard from '@/locales/en/dashboard.json'
import enServers from '@/locales/en/servers.json'
import enTerminal from '@/locales/en/terminal.json'
import enSettings from '@/locales/en/settings.json'
import enLogin from '@/locales/en/login.json'
import enStatus from '@/locales/en/status.json'

import zhCommon from '@/locales/zh/common.json'
import zhDashboard from '@/locales/zh/dashboard.json'
import zhServers from '@/locales/zh/servers.json'
import zhTerminal from '@/locales/zh/terminal.json'
import zhSettings from '@/locales/zh/settings.json'
import zhLogin from '@/locales/zh/login.json'
import zhStatus from '@/locales/zh/status.json'

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      en: {
        common: enCommon,
        dashboard: enDashboard,
        servers: enServers,
        terminal: enTerminal,
        settings: enSettings,
        login: enLogin,
        status: enStatus,
      },
      zh: {
        common: zhCommon,
        dashboard: zhDashboard,
        servers: zhServers,
        terminal: zhTerminal,
        settings: zhSettings,
        login: zhLogin,
        status: zhStatus,
      },
    },
    fallbackLng: 'en',
    defaultNS: 'common',
    detection: {
      order: ['localStorage', 'navigator'],
      cacheUserLanguage: true,
    },
    interpolation: {
      escapeValue: false, // React handles XSS
    },
  })

export default i18n
```

Entry point: add `import '@/lib/i18n'` in `src/main.tsx`.

### Type Safety

File: `src/lib/i18n.d.ts`

```typescript
import type enCommon from '@/locales/en/common.json'
import type enDashboard from '@/locales/en/dashboard.json'
import type enServers from '@/locales/en/servers.json'
import type enTerminal from '@/locales/en/terminal.json'
import type enSettings from '@/locales/en/settings.json'
import type enLogin from '@/locales/en/login.json'
import type enStatus from '@/locales/en/status.json'

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
```

English JSON files serve as the source of truth. Missing keys in zh fall back to English at runtime.

### Translation File Structure

```
src/locales/
├── en/
│   ├── common.json        # Shared: navigation, buttons, statuses, errors
│   ├── dashboard.json     # Dashboard page
│   ├── servers.json       # Server list + detail pages
│   ├── terminal.json      # Terminal page
│   ├── settings.json      # Settings main + all sub-pages
│   ├── login.json         # Login page
│   └── status.json        # Public status page
└── zh/
    ├── common.json
    ├── dashboard.json
    ├── servers.json
    ├── terminal.json
    ├── settings.json
    ├── login.json
    └── status.json
```

Namespace mapping:
- `common` — cross-page shared text (sidebar nav items, generic buttons like "Save"/"Cancel"/"Delete", statuses "Online"/"Offline", generic error messages)
- Other namespaces map 1:1 to route modules
- `settings.json` merges all settings sub-pages, using key prefixes to distinguish: `"security.title"`, `"alerts.rules"`, `"users.add_user"`, etc.

### Key Naming Convention

Flat dot-separated keys within each namespace JSON:

```json
// en/dashboard.json
{
  "title": "Dashboard",
  "servers_online": "{{online}} of {{total}} servers online",
  "avg_cpu": "Avg CPU",
  "avg_memory": "Avg Memory",
  "total_bandwidth": "Total Bandwidth",
  "online": "Online",
  "healthy": "Healthy",
  "no_data": "No data",
  "no_servers_title": "No servers connected yet",
  "no_servers_description": "Servers will appear here once they connect via the agent",
  "ungrouped": "Ungrouped"
}

// zh/dashboard.json
{
  "title": "仪表盘",
  "servers_online": "{{total}} 台服务器中 {{online}} 台在线",
  "avg_cpu": "平均 CPU",
  "avg_memory": "平均内存",
  "total_bandwidth": "总带宽",
  "online": "在线",
  "healthy": "健康",
  "no_data": "暂无数据",
  "no_servers_title": "暂无服务器连接",
  "no_servers_description": "服务器通过 Agent 连接后将显示在此处",
  "ungrouped": "未分组"
}
```

### Component Usage Pattern

```tsx
// Page-level: specify namespace
const { t } = useTranslation('dashboard')
<h1>{t('title')}</h1>
<p>{t('servers_online', { online: onlineCount, total: servers.length })}</p>

// Cross-namespace reference
const { t } = useTranslation(['dashboard', 'common'])
<Button>{t('common:save')}</Button>
```

### Language Switcher

Location: `src/components/layout/header.tsx`, next to the existing theme toggle button.

```tsx
import { useTranslation } from 'react-i18next'

function LanguageSwitcher() {
  const { i18n } = useTranslation()
  const toggle = () => i18n.changeLanguage(i18n.language === 'en' ? 'zh' : 'en')

  return (
    <Button variant="ghost" size="icon" onClick={toggle}>
      {i18n.language === 'en' ? '中文' : 'EN'}
    </Button>
  )
}
```

- Simple toggle for two languages (no dropdown needed)
- `changeLanguage()` triggers re-render of all components using `useTranslation()`
- Language preference auto-persisted to localStorage by the LanguageDetector plugin

## File Change Summary

### New Files (16)

| File | Purpose |
|------|---------|
| `src/lib/i18n.ts` | i18next initialization and configuration |
| `src/lib/i18n.d.ts` | TypeScript type declarations for translation keys |
| `src/locales/en/common.json` | English shared translations |
| `src/locales/en/dashboard.json` | English dashboard translations |
| `src/locales/en/servers.json` | English servers translations |
| `src/locales/en/terminal.json` | English terminal translations |
| `src/locales/en/settings.json` | English settings translations |
| `src/locales/en/login.json` | English login translations |
| `src/locales/en/status.json` | English status page translations |
| `src/locales/zh/common.json` | Chinese shared translations |
| `src/locales/zh/dashboard.json` | Chinese dashboard translations |
| `src/locales/zh/servers.json` | Chinese servers translations |
| `src/locales/zh/terminal.json` | Chinese terminal translations |
| `src/locales/zh/settings.json` | Chinese settings translations |
| `src/locales/zh/login.json` | Chinese login translations |
| `src/locales/zh/status.json` | Chinese status page translations |

### Modified Files (~20)

| File | Change |
|------|--------|
| `src/main.tsx` | Add `import '@/lib/i18n'` |
| `src/components/layout/header.tsx` | Add LanguageSwitcher component |
| `src/components/layout/sidebar.tsx` | Extract nav item labels to `common` namespace |
| `src/routes/login.tsx` | Extract text to `login` namespace |
| `src/routes/status.tsx` | Extract text to `status` namespace |
| `src/routes/_authed/index.tsx` | Extract text to `dashboard` namespace |
| `src/routes/_authed/servers/index.tsx` | Extract text to `servers` namespace |
| `src/routes/_authed/servers/$id.tsx` | Extract text to `servers` namespace |
| `src/routes/_authed/terminal.$serverId.tsx` | Extract text to `terminal` namespace |
| `src/routes/_authed/settings/index.tsx` | Extract text to `settings` namespace |
| `src/routes/_authed/settings/security.tsx` | Extract text to `settings` namespace |
| `src/routes/_authed/settings/users.tsx` | Extract text to `settings` namespace |
| `src/routes/_authed/settings/notifications.tsx` | Extract text to `settings` namespace |
| `src/routes/_authed/settings/alerts.tsx` | Extract text to `settings` namespace |
| `src/routes/_authed/settings/api-keys.tsx` | Extract text to `settings` namespace |
| `src/routes/_authed/settings/capabilities.tsx` | Extract text to `settings` namespace |
| `src/routes/_authed/settings/audit-logs.tsx` | Extract text to `settings` namespace |
| `src/routes/_authed/settings/ping-tasks.tsx` | Extract text to `settings` namespace |
| `src/routes/_authed/settings/tasks.tsx` | Extract text to `settings` namespace |
| `src/components/server/server-card.tsx` | Extract text to `servers` namespace |
| `src/components/server/server-edit-dialog.tsx` | Extract text to `servers` namespace |

### Unchanged

- `src/lib/utils.ts` — formatting functions remain as-is
- `src/lib/api-client.ts` — no user-visible text
- `src/lib/ws-client.ts` — no user-visible text
- `src/hooks/` — hooks return data, not UI text
- `src/components/ui/` — shadcn base components have no business text
- Test files — existing tests do not depend on UI text content

## Testing

- Existing 72 vitest tests should pass without changes (they test logic, not UI text)
- Manual verification: switch language and confirm all pages render correctly in both en and zh
- TypeScript `bun run typecheck` validates all translation keys exist
