import type { Translations } from 'fumadocs-ui/i18n'

import type { DocsLanguage } from './i18n'

/**
 * fumadocs-ui's interface text by language: control names, the search box, the table of contents. English keeps the
 * text fumadocs-ui ships, which a language falls back to for any key it leaves out. The keys after `editOnGithub` exist
 * through patches/fumadocs-ui@16.6.16.patch, which routes the labels fumadocs-ui hard-codes in English through these
 * translations.
 */
export const uiTranslations: Record<DocsLanguage, Partial<Translations> & { displayName: string }> = {
  en: {
    displayName: 'English'
  },
  zh: {
    displayName: '中文',
    search: '搜索文档',
    searchNoResult: '没有找到结果',
    toc: '本页目录',
    tocNoHeadings: '本页没有标题',
    lastUpdate: '最后更新于',
    chooseLanguage: '选择语言',
    nextPage: '下一页',
    previousPage: '上一页',
    chooseTheme: '主题',
    editOnGithub: '在 GitHub 上编辑',
    openSearch: '打开搜索',
    openSidebar: '打开侧边栏',
    closeSidebar: '关闭侧边栏',
    collapseSidebar: '收起侧边栏',
    expandSidebar: '展开侧边栏',
    toggleTheme: '切换主题',
    toggleMenu: '切换菜单',
    mainNavigation: '主要',
    copyText: '复制代码',
    copiedText: '已复制',
    copyMarkdown: '复制 Markdown',
    openPageActions: '打开',
    openInGitHub: '在 GitHub 上打开',
    viewAsMarkdown: '查看 Markdown',
    openInApp: '在 {app} 中打开',
    askAboutPage: '阅读 {url}，我想就这个页面提问。'
  }
}

/**
 * The search dialog's text that fumadocs-ui has no key for. Its close button showed its key, ESC, and that was all its
 * name said, so the name keeps the key it shows. A screen reader hears the result count once results arrive.
 */
export const searchDialogText: Record<DocsLanguage, { closeSearch: string; resultCount: (count: number) => string }> = {
  en: {
    closeSearch: 'Close search (Esc)',
    resultCount: (count) => (count === 1 ? '1 result' : `${count} results`)
  },
  zh: {
    closeSearch: '关闭搜索（Esc）',
    resultCount: (count) => `${count} 个结果`
  }
}
