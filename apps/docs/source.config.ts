import { remarkStructureDefaultOptions } from 'fumadocs-core/mdx-plugins'
import { defineConfig, defineDocs } from 'fumadocs-mdx/config'

export const docs = defineDocs({
  dir: 'content/docs',
  docs: {
    postprocess: {
      includeProcessedMarkdown: true
    }
  }
})

export default defineConfig({
  mdxOptions: {
    remarkStructureOptions: {
      // Search indexed each <Card> as its raw source, a result that opened the page holding the card instead of the
      // page it links to. That page is found under its own title.
      mdxTypes: (node) => node.name !== 'Card' && remarkStructureDefaultOptions.mdxTypes(node)
    }
  }
})
