import defaultMdxComponents from 'fumadocs-ui/mdx'
import { Children, type ComponentProps, cloneElement, isValidElement, type ReactNode } from 'react'

// Inline elements Markdown produces, which a label can hold, and the components that render links and images.
const phrasingElements = new Set('a abbr b br code del em i img kbd mark s span strong sub sup u'.split(' '))
const phrasingComponents = new Set<unknown>([defaultMdxComponents.a, defaultMdxComponents.img])

function isPhrasing(node: ReactNode) {
  if (!isValidElement(node)) {
    return true
  }
  return typeof node.type === 'string' ? phrasingElements.has(node.type) : phrasingComponents.has(node.type)
}

/** Nodes that start with a checkbox, with the checkbox and the inline content after it wrapped in a label. */
function labelCheckbox(nodes: ReactNode[]): ReactNode[] | undefined {
  const [checkbox] = nodes
  if (!(isValidElement(checkbox) && checkbox.type === 'input')) {
    return
  }
  const end = nodes.findIndex((node, index) => index > 0 && !isPhrasing(node))
  const inline = end === -1 ? nodes.length : end
  return [
    // biome-ignore lint/a11y/noLabelWithoutControl: the checkbox is among the label's children
    <label key="label">{nodes.slice(0, inline)}</label>,
    ...nodes.slice(inline)
  ]
}

/**
 * A list item. GFM renders the checkbox of a task list item (`- [ ] ...`) without a name, so the inline content that
 * follows it becomes its label, up to a code block, table, quote or nested list. In a loose list the checkbox starts
 * the item's first paragraph, and the label goes there.
 */
export function ListItem({ children, ...props }: ComponentProps<'li'>) {
  if (!props.className?.includes('task-list-item')) {
    return <li {...props}>{children}</li>
  }
  const nodes = Children.toArray(children)
  const labeled = labelCheckbox(nodes)
  if (labeled) {
    return <li {...props}>{labeled}</li>
  }
  const start = nodes.findIndex((node) => !(typeof node === 'string' && node.trim() === ''))
  const paragraph = nodes[start]
  if (isValidElement<{ children?: ReactNode }>(paragraph) && paragraph.type === 'p') {
    const content = labelCheckbox(Children.toArray(paragraph.props.children))
    if (content) {
      nodes[start] = cloneElement(paragraph, undefined, ...content)
    }
  }
  return <li {...props}>{nodes}</li>
}
