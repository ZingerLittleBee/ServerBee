import { docs } from 'collections/server'
import type { Folder, Item, Node, Root } from 'fumadocs-core/page-tree'
import { type InferPageType, loader } from 'fumadocs-core/source'
import { lucideIconsPlugin } from 'fumadocs-core/source/lucide-icons'

import { i18n } from './i18n'

export const source = loader({
  i18n,
  source: docs.toFumadocsSource(),
  baseUrl: '/docs',
  plugins: [lucideIconsPlugin()]
})

export async function getLLMText(page: InferPageType<typeof source>) {
  const processed = await page.data.getText('processed')

  return `# ${page.data.title}

${processed}`
}

function localizeNode(node: Node, lang: string): Node {
  switch (node.type) {
    case 'page':
      return { ...node, url: `/${lang}${node.url}` } satisfies Item
    case 'folder':
      return {
        ...node,
        children: node.children.map((n: Node) => localizeNode(n, lang)),
        ...(node.index ? { index: { ...node.index, url: `/${lang}${node.index.url}` } satisfies Item } : {})
      } satisfies Folder
    default:
      return node
  }
}

export function localizePageTree(tree: Root, lang: string): Root {
  return {
    ...tree,
    children: tree.children.map((n: Node) => localizeNode(n, lang))
  }
}
