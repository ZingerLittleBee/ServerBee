import { Children, type ComponentProps, isValidElement } from 'react'

// Block content, which a label cannot hold.
const blockElements = new Set(['blockquote', 'div', 'figure', 'ol', 'p', 'pre', 'table', 'ul'])

/**
 * A list item. GFM renders the checkbox of a task list item (`- [ ] ...`) without a name, so the item's text, when it
 * is all inline, becomes the checkbox's label.
 */
export function ListItem({ children, ...props }: ComponentProps<'li'>) {
  const labeled =
    props.className?.includes('task-list-item') &&
    Children.toArray(children).every(
      (child) => !(isValidElement(child) && typeof child.type === 'string' && blockElements.has(child.type))
    )
  if (!labeled) {
    return <li {...props}>{children}</li>
  }
  return (
    <li {...props}>
      {/* biome-ignore lint/a11y/noLabelWithoutControl: the checkbox is among the item's children */}
      <label>{children}</label>
    </li>
  )
}
