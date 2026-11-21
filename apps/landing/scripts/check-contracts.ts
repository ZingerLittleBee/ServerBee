/*
 * Checks what the landing page says against the rest of the repository. The build cannot see any of it: the
 * documentation is a separate app, and the installer and the license live at the workspace root.
 *
 *   bun run check:contracts     # in apps/landing
 */

import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import GithubSlugger from 'github-slugger'
import { landingCopy } from '../src/i18n/copy'
import { DOCS_SITE, docsPages, featureOrder, installCommands, LANGS, type Lang } from '../src/lib/site'

const LANDING = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const REPOSITORY = resolve(LANDING, '../..')
const DOCS_CONTENT = join(REPOSITORY, 'apps/docs/content/docs')
const COPY = 'apps/landing/src/i18n/copy.ts'
const SCRIPT = 'apps/landing/scripts/check-contracts.ts'

const frontmatter = /^---\n[\s\S]*?\n---\n/
const fencedCode = /^(```|~~~)[\s\S]*?^\1.*$/gm
const heading = /^#{1,6}[ \t]+(.+?)[ \t]*$/gm
const customHeadingId = /\s*\[#([^\]]+)\]$/
const markdownLink = /\[([^\]]*)\]\([^)]*\)/g
const docsLink = /^\/(\w+)\/docs\/([\w-]+)$/
const literalIosVersion = /\biOS \d/
const installerFlag = / (--[a-z-]+)/g
const installerMethod = /--method (\w+)/
/** The methods deploy/install.sh names in its usage line. */
const INSTALL_METHODS = ['binary', 'docker']

/** The documentation pages the copy links to, in either language. */
const linkedPages = new Set<string>()

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(message)
  }
}

function text(path: string): string {
  return readFileSync(path, 'utf8')
}

function docsFile(lang: string, page: string): string {
  return join(DOCS_CONTENT, lang, `${page}.mdx`)
}

/**
 * The ids the documentation gives a page's headings: GitHub's slugs, numbered when repeated, or a custom `[#id]`.
 * A `#` line in a code block is not a heading.
 */
function headingIds(mdx: string): Set<string> {
  const slugger = new GithubSlugger()
  const ids = new Set<string>()
  for (const [, title] of mdx.replace(frontmatter, '').replace(fencedCode, '').matchAll(heading)) {
    const custom = customHeadingId.exec(title)?.[1]
    ids.add(custom ?? slugger.slug(title.replace(markdownLink, '$1').replace(/[`*]/g, '')))
  }
  return ids
}

/** Every string in a locale's copy. */
function strings(value: unknown, found: string[] = []): string[] {
  if (typeof value === 'string') {
    found.push(value)
  } else if (typeof value === 'object' && value !== null) {
    for (const item of Object.values(value)) {
      strings(item, found)
    }
  }
  return found
}

function checkDocsLink(lang: Lang, href: string): void {
  const url = new URL(href)
  const [, linkLang, page] = docsLink.exec(url.pathname) ?? []
  invariant(page, `The ${lang} copy links to ${href}, which is not a documentation page: update ${COPY}`)
  invariant(linkLang === lang, `The ${lang} copy links to the ${linkLang} documentation at ${href}: update ${COPY}`)
  invariant(
    (docsPages as readonly string[]).includes(page),
    `The ${lang} copy links to ${href}, but docsPages in apps/landing/src/lib/site.ts does not list "${page}"`
  )
  linkedPages.add(page)
  if (url.hash) {
    const id = decodeURIComponent(url.hash.slice(1))
    const ids = headingIds(text(docsFile(lang, page)))
    invariant(
      ids.has(id),
      `The ${lang} copy links to the heading "${id}", which ${lang}/${page}.mdx does not have: update ${COPY}. ` +
        `The page has: ${[...ids].join(', ')}`
    )
  }
}

// Every documentation page the landing can link to exists in both locales.
for (const page of docsPages) {
  for (const lang of LANGS) {
    invariant(
      existsSync(docsFile(lang, page)),
      `The landing links to ${lang}/${page}, but apps/docs/content/docs/${lang}/${page}.mdx is missing: ` +
        'update docsPages in apps/landing/src/lib/site.ts'
    )
  }
}

const cargo = text(join(REPOSITORY, 'Cargo.toml'))
const license = cargo.match(/^license\s*=\s*"([^"]+)"/m)?.[1]
invariant(license, 'Unable to read the workspace license from Cargo.toml')

for (const lang of LANGS) {
  const copy = strings(landingCopy[lang])

  const docsLinks = copy.filter((value) => value.startsWith(DOCS_SITE))
  invariant(docsLinks.length > 0, `Found no documentation links in the ${lang} copy: update ${SCRIPT}`)
  for (const href of docsLinks) {
    checkDocsLink(lang, href)
  }

  // An in-page link targets a feature section, whose id is its key.
  for (const href of copy.filter((value) => value.startsWith('#'))) {
    invariant(
      (featureOrder[lang] as string[]).includes(href.slice(1)),
      `The ${lang} copy links to ${href}, which is not a feature section: update ${COPY}`
    )
  }

  invariant(
    copy.some((value) => value.includes(license)),
    `The ${lang} copy does not name the workspace license ${license}: update ${COPY}`
  )
  for (const value of copy) {
    invariant(!/\bMIT\b/.test(value), `The ${lang} copy claims an MIT license, the workspace is ${license}: "${value}"`)
    // src/lib/release.ts reads the deployment target from apps/ios/project.yml.
    invariant(!literalIosVersion.test(value), `The ${lang} copy spells out an iOS version, use {ios}: "${value}"`)
  }
  // The same file reads the release from Cargo.toml.
  invariant(
    landingCopy[lang].hero.release.includes('{version}'),
    `The ${lang} release announcement does not use {version}: update hero.release in ${COPY}`
  )
}

for (const page of docsPages) {
  invariant(linkedPages.has(page), `No copy links to the documentation page "${page}": remove it from docsPages`)
}

// The commands the page tells visitors to run use options the installer accepts.
const installer = text(join(REPOSITORY, 'deploy/install.sh'))
for (const [name, command] of Object.entries(installCommands)) {
  invariant(
    command.includes('/main/deploy/install.sh'),
    `The ${name} command no longer downloads deploy/install.sh: update this check in ${SCRIPT}`
  )
  for (const [, flag] of command.matchAll(installerFlag)) {
    invariant(
      installer.includes(`${flag})`),
      `The ${name} command passes ${flag}, which deploy/install.sh does not parse`
    )
  }
  const method = installerMethod.exec(command)?.[1]
  if (method) {
    invariant(
      installer.includes(`[--method ${INSTALL_METHODS.join('|')}]`),
      `deploy/install.sh no longer documents "--method ${INSTALL_METHODS.join('|')}": update INSTALL_METHODS in ${SCRIPT}`
    )
    invariant(
      INSTALL_METHODS.includes(method),
      `The ${name} command installs with --method ${method}, which deploy/install.sh does not offer`
    )
  }
}

console.log('Landing contracts hold.')
