import { createFileRoute, notFound, Outlet, redirect } from '@tanstack/react-router'

import { type DocsLanguage, i18n, isDocsLanguage, pathLanguage } from '@/lib/i18n'
import { docsSiteName, notFoundHead } from '@/lib/site'

const subtagSeparator = /[-_]/
const firstSegment = /^\/[^/?#]*/

// The Chinese pages were published under /cn until the locale was renamed zh, and installers before v1.0.0-beta.1 print
// /cn links. A Map, because an object lookup would also match keys such as "constructor".
const renamedLanguages = new Map<string, DocsLanguage>([['cn', 'zh']])

/** The site language a first path segment names: "zh-CN", "ZH" and the former "cn" mean zh. */
function siteLanguage(segment: string): DocsLanguage | undefined {
  const tag = segment.toLowerCase()
  const primary = tag.split(subtagSeparator)[0]
  return renamedLanguages.get(tag) ?? (isDocsLanguage(primary) ? primary : undefined)
}

export const Route = createFileRoute('/$lang')({
  beforeLoad: ({ location, params }) => {
    if (isDocsLanguage(params.lang)) {
      return
    }
    // The redirects keep the rest of the URL from location.href, which keeps the request's percent-encoding:
    // location.pathname is decoded, and a Location header with a character above U+00FF throws (a 500) or goes out as
    // Latin-1.
    if (params.lang === 'docs') {
      // The docs without a language, as / is the landing without one. Sending it to the default language is a guess,
      // so the redirect is temporary.
      throw redirect({ href: `/${i18n.defaultLanguage}${location.href}` })
    }
    const lang = siteLanguage(params.lang)
    if (!lang) {
      // A 404 rather than a redirect to the English landing, which crawlers report as a soft 404, and which answered
      // paths such as /apple-touch-icon.png with an HTML page.
      throw notFound()
    }
    throw redirect({ href: location.href.replace(firstSegment, `/${lang}`), statusCode: 308 })
  },
  // The title of a page without its own, in the language the 404 page takes from the path. An unknown language is a
  // 404 here, and so is a path under a language that no route matches (_notFound).
  head: ({ match }) => {
    const lang = pathLanguage(match.pathname)
    return match.status === 'notFound' || match._notFound
      ? notFoundHead(lang)
      : { meta: [{ title: docsSiteName[lang] }] }
  },
  component: () => <Outlet />
})
