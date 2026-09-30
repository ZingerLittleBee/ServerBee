import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared'

import { i18n } from './i18n'

export const gitConfig = {
  user: 'ZingerLittleBee',
  repo: 'ServerBee',
  branch: 'main'
}

export function baseOptions(lang: string = i18n.defaultLanguage): BaseLayoutProps {
  return {
    nav: {
      title: 'ServerBee',
      url: `/${lang}`
    },
    githubUrl: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
    i18n
  }
}
