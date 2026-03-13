import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared'

import { i18n } from './i18n'

export const gitConfig = {
  user: 'zingerbee',
  repo: 'ServerBee',
  branch: 'main'
}

export function baseOptions(_lang?: string): BaseLayoutProps {
  return {
    nav: {
      title: 'ServerBee'
    },
    githubUrl: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
    i18n
  }
}
