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
    rehypeCodeOptions: {
      // fumadocs' default themes, named here because the replacements below are theirs.
      themes: { light: 'github-light', dark: 'github-dark' },
      // Token colors of these themes that fall under 4.5:1 on the code block backgrounds (#f1f1f1 light, #191919
      // dark), replaced by the colors of GitHub's newer themes for the same tokens, or by a darker step where those
      // are still too light.
      colorReplacements: {
        'github-light': { '#6a737d': '#57606a', '#d73a49': '#cf222e', '#22863a': '#116329', '#e36209': '#953800' },
        'github-dark': { '#6a737d': '#8b949e' }
      }
    },
    remarkStructureOptions: {
      // Search indexed each <Card> as its raw source, a result that opened the page holding the card instead of the
      // page it links to. That page is found under its own title.
      mdxTypes: (node) => node.name !== 'Card' && remarkStructureDefaultOptions.mdxTypes(node)
    }
  }
})
