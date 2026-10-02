import { Link } from '@tanstack/react-router'
import type { Framework } from 'fumadocs-core/framework'

type FrameworkLinkProps = Parameters<NonNullable<Framework['Link']>>[0]

/**
 * fumadocs renders its internal links (MDX content, sidebar, navigation, page footer) as router links with
 * `to={href}`, which has two problems here.
 *
 * The router reads a fragment in `to` as part of the path: a link to a heading on the same page resolves to
 * `/en/docs/page/#id` and drops the fragment when followed, and a link to another page's heading preloads a page
 * named `page#id`, which does not exist. Same-page fragments stay plain links, like the table of contents; any other
 * fragment goes to the router as its `hash`.
 *
 * The router also marks a link active, with aria-current="page", whenever the current path starts with its path, so
 * the home and docs index links were announced as the current page everywhere. Only the page itself is current, and
 * a query string such as ?ref=github does not change which page that is.
 */
export function FrameworkLink({ href = '#', prefetch = true, ...props }: FrameworkLinkProps) {
  if (href.startsWith('#')) {
    return <a href={href} {...props} />
  }
  const at = href.indexOf('#')
  return (
    <Link
      {...props}
      activeOptions={{ exact: true, includeSearch: false }}
      hash={at === -1 ? undefined : href.slice(at + 1)}
      preload={prefetch ? 'intent' : false}
      to={at === -1 ? href : href.slice(0, at)}
    />
  )
}
