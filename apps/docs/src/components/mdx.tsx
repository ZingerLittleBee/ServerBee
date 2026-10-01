import defaultMdxComponents from 'fumadocs-ui/mdx'
import type { MDXComponents } from 'mdx/types'

import { Table } from './table'

export function getMDXComponents(components?: MDXComponents) {
  return {
    ...defaultMdxComponents,
    table: Table,
    ...components
  } satisfies MDXComponents
}

export const useMDXComponents = getMDXComponents

declare global {
  type MDXProvidedComponents = ReturnType<typeof getMDXComponents>
}
