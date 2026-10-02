import { readdir, readFile, realpath } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { basename, dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { getTableOfContents } from 'fumadocs-core/content/toc'
import { remarkGfm } from 'fumadocs-core/mdx-plugins/remark-gfm'
import { defaultTranslations, type Translations } from 'fumadocs-ui/i18n'

import { docsPages, landingCopy } from '../src/components/landing/translations'
import { searchDialogText, uiTranslations } from '../src/lib/ui-translations'

const docsApp = resolve(fileURLToPath(new URL('..', import.meta.url)))
const repository = resolve(docsApp, '../..')
const contentRoot = join(docsApp, 'content/docs')
const locales = ['en', 'zh'] as const
const markdownTableDivider = /^\|(?:\s*:?-+:?\s*\|)+$/
const numericTableCell = /^[\d,+~\s]+$/
const landingTranslations = 'apps/docs/src/components/landing/translations.ts'
const iosVersionMention = /\biOS (\d+)/g
const darkClassRoot = /:root\.dark(?![\w-])/g
const darkMediaRoot = ':root:not(.light):not(.dark)'
const darkMediaQuery = /^@media \(prefers-color-scheme: ?dark\)$/
// remark, without a frontmatter plugin, reads a page's frontmatter as a heading.
const frontmatter = /^---\n[\s\S]*?\n---\n/
const hslNotation = /^hsla?\(([\d.]+),\s*([\d.]+)%,\s*([\d.]+)%(?:,\s*([\d.]+)(%?))?\)$/
const cssImport = /@import\s+([^;]+);/g
const cssImportTarget = /^(["'])([^"']+)\1$/
const packagePath = /^((?:@[^/]+\/)?[^/]+)\/(.+)$/
const shellCodeBlock = /^```(?:bash|sh)\n([\s\S]*?)^```/gm
const allocatorAssignment = /(?:MALLOC_ARENA_MAX|Environment=MALLOC_ARENA_MAX)=/

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(message)
  }
}

function text(path: string): Promise<string> {
  return readFile(path, 'utf8')
}

function normalizedProse(markdown: string): string {
  return markdown.replace(/[`*]/g, '').replace(/\s+/g, ' ').trim()
}

function firstMarkdownTable(markdown: string): string[][] {
  const lines = markdown.split('\n')
  for (let index = 0; index < lines.length - 1; index += 1) {
    if (!(lines[index].startsWith('|') && markdownTableDivider.test(lines[index + 1]))) {
      continue
    }

    const rows: string[][] = []
    for (let row = index; row < lines.length && lines[row].startsWith('|'); row += 1) {
      if (row === index + 1) {
        continue
      }
      rows.push(
        lines[row]
          .slice(1, -1)
          .split('|')
          .map((cell) => cell.trim())
      )
    }
    return rows
  }
  return []
}

/**
 * The ids of a page's headings, computed as fumadocs-mdx computes them (remark-heading, after GFM): GitHub's slugs,
 * numbered when repeated, or a custom `[#id]`. A `#` line in a code block is not a heading.
 */
async function headingIds(mdx: string): Promise<Set<string>> {
  const toc = await getTableOfContents(mdx.replace(frontmatter, ''), [remarkGfm])
  return new Set(toc.map((item) => item.url.slice(1)))
}

interface LandingDocsLink {
  hash?: string
  lang: string
  page: string
}

interface LandingCopyParts {
  links: LandingDocsLink[]
  strings: string[]
}

/** Every string and every documentation link (a docsPath() value: { lang, page, hash? }) in the landing copy. */
function collectLandingCopy(value: unknown, parts: LandingCopyParts): LandingCopyParts {
  if (typeof value === 'string') {
    parts.strings.push(value)
  } else if (typeof value === 'object' && value !== null) {
    if ('lang' in value && 'page' in value && typeof value.lang === 'string' && typeof value.page === 'string') {
      const hash = 'hash' in value && typeof value.hash === 'string' ? value.hash : undefined
      parts.links.push({ hash, lang: value.lang, page: value.page })
    } else {
      for (const item of Object.values(value)) {
        collectLandingCopy(item, parts)
      }
    }
  }
  return parts
}

/** The real directory of package `name` as Node's module lookup finds it from `directory`. */
async function packageDirectory(name: string, directory: string): Promise<string | undefined> {
  for (const lookup of createRequire(join(directory, 'package.json')).resolve.paths(name) ?? []) {
    const found = await realpath(join(lookup, name)).catch(() => undefined)
    if (found) {
      return found
    }
  }
  return undefined
}

/** The index just past the CSS string that opens with the quote at `start`. */
function cssStringEnd(css: string, start: number): number {
  let index = start + 1
  while (index < css.length && css[index] !== css[start]) {
    index += css[index] === '\\' ? 2 : 1
  }
  return index + 1
}

/** CSS with its comments replaced by spaces; comment markers inside strings are kept. */
function withoutCssComments(css: string): string {
  let result = ''
  let index = 0
  while (index < css.length) {
    let next = index + 1
    if (css[index] === '"' || css[index] === "'") {
      next = cssStringEnd(css, index)
      result += css.slice(index, next)
    } else if (css.startsWith('/*', index)) {
      const end = css.indexOf('*/', index + 2)
      next = end === -1 ? css.length : end + 2
      result += ' '
    } else {
      result += css[index]
    }
    index = next
  }
  return result
}

interface CssBlock {
  body: string
  children: CssBlock[]
  /** The body without the blocks nested in it. */
  ownBody: string
  /** The selector list or at-rule prelude, whitespace collapsed. */
  prelude: string
}

/** The block tree of a stylesheet. Braces inside strings do not count. */
function cssBlocks(source: string): CssBlock[] {
  const css = withoutCssComments(source)
  const top: CssBlock[] = []
  const open: { children: CssBlock[]; ownBody: string; ownStart: number; prelude: string; start: number }[] = []
  let statementStart = 0
  let index = 0
  while (index < css.length) {
    const char = css[index]
    let next = index + 1
    if (char === '"' || char === "'") {
      next = cssStringEnd(css, index)
    } else if (char === '{') {
      const parent = open.at(-1)
      if (parent) {
        parent.ownBody += css.slice(parent.ownStart, statementStart)
      }
      const prelude = css.slice(statementStart, index).replace(/\s+/g, ' ').trim()
      open.push({ children: [], ownBody: '', ownStart: next, prelude, start: next })
      statementStart = next
    } else if (char === '}') {
      const block = open.pop()
      invariant(block, 'Unbalanced braces in a stylesheet')
      const parent = open.at(-1)
      if (parent) {
        parent.ownStart = next
      }
      const siblings = parent?.children ?? top
      siblings.push({
        body: css.slice(block.start, index),
        children: block.children,
        ownBody: block.ownBody + css.slice(block.ownStart, index),
        prelude: block.prelude
      })
      statementStart = next
    } else if (char === ';') {
      statementStart = next
    }
    index = next
  }
  invariant(open.length === 0, 'Unclosed block in a stylesheet')
  return top
}

/** A style block's declarations by property, values whitespace collapsed. */
function cssDeclarations(body: string): Map<string, string> {
  const declarations = new Map<string, string>()
  for (const declaration of body.split(';')) {
    const colon = declaration.indexOf(':')
    if (colon > 0) {
      declarations.set(
        declaration.slice(0, colon).trim(),
        declaration
          .slice(colon + 1)
          .replace(/\s+/g, ' ')
          .trim()
      )
    }
  }
  return declarations
}

/** Every block of a block tree, with the preludes of the blocks it is nested in. */
function* nestedBlocks(blocks: CssBlock[], outer: string[] = []): Generator<[CssBlock, string[]]> {
  for (const block of blocks) {
    yield [block, outer]
    yield* nestedBlocks(block.children, [...outer, block.prelude])
  }
}

/** The file of a stylesheet that `from` imports by a relative path or a package's path. */
async function importedStylesheet(path: string, from: string): Promise<string> {
  if (path.startsWith('.')) {
    return resolve(dirname(from), path)
  }
  const match = path.match(packagePath)
  const directory = match ? await packageDirectory(match[1], dirname(from)) : undefined
  invariant(match && directory, `${relative(repository, from)} imports ${path}, which apps/docs cannot resolve`)
  return join(directory, match[2])
}

/**
 * The blocks of a stylesheet and of the stylesheets it imports, in the order the cascade reads them. Tailwind's own
 * stylesheet is left out: it sets no fumadocs colors.
 */
async function stylesheetBlocks(file: string): Promise<{ blocks: CssBlock[]; file: string }[]> {
  const css = await text(file)
  const sheets: { blocks: CssBlock[]; file: string }[] = []
  for (const [statement, target] of withoutCssComments(css).matchAll(cssImport)) {
    const path = target.trim().match(cssImportTarget)?.[2]
    invariant(
      path,
      `${relative(repository, file)}: unexpected ${statement}: update stylesheetBlocks in apps/docs/scripts/check-contracts.ts`
    )
    if (path !== 'tailwindcss') {
      sheets.push(...(await stylesheetBlocks(await importedStylesheet(path, file))))
    }
  }
  sheets.push({ blocks: cssBlocks(css), file })
  return sheets
}

/** An sRGB color, channels and alpha from 0 to 1. */
type Rgba = [number, number, number, number]

/** An hsl() or hsla() color, the notation fumadocs' themes use. */
function hslColor(value: string | undefined): Rgba {
  const match = value?.match(hslNotation)
  invariant(match, `Unexpected color ${value ?? '<missing>'}: update hslColor in apps/docs/scripts/check-contracts.ts`)
  const [hue, saturation, lightness] = [Number(match[1]), Number(match[2]) / 100, Number(match[3]) / 100]
  const channel = (n: number) => {
    const k = (n + hue / 30) % 12
    return lightness - saturation * Math.min(lightness, 1 - lightness) * Math.max(-1, Math.min(k - 3, 9 - k, 1))
  }
  const alpha = match[4] === undefined ? 1 : Number(match[4]) / (match[5] ? 100 : 1)
  return [channel(0), channel(8), channel(4), alpha]
}

/** A color painted over the opaque `ground`. */
function composite([red, green, blue, alpha]: Rgba, ground: Rgba): Rgba {
  return [
    red * alpha + ground[0] * (1 - alpha),
    green * alpha + ground[1] * (1 - alpha),
    blue * alpha + ground[2] * (1 - alpha),
    1
  ]
}

/** The WCAG 2 contrast ratio of two opaque colors. */
function contrastRatio(first: Rgba, second: Rgba): number {
  const luminance = ([red, green, blue]: Rgba) => {
    const linear = (channel: number) => (channel <= 0.040_45 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4)
    return 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
  }
  const [lighter, darker] = [luminance(first), luminance(second)].sort((a, b) => b - a)
  return (lighter + 0.05) / (darker + 0.05)
}

const localePages = new Map<string, Set<string>>()
for (const locale of locales) {
  const files = (await readdir(join(contentRoot, locale)))
    .filter((file) => file.endsWith('.mdx'))
    .map((file) => basename(file, '.mdx'))
  localePages.set(locale, new Set(files))
}

function pagesFor(locale: string): Set<string> {
  const pages = localePages.get(locale)
  invariant(pages, `Unknown documentation locale: ${locale}`)
  return pages
}

invariant(
  JSON.stringify([...pagesFor('en')].sort()) === JSON.stringify([...pagesFor('zh')].sort()),
  'English and Chinese documentation page sets differ'
)

for (const locale of locales) {
  const meta = JSON.parse(await text(join(contentRoot, locale, 'meta.json'))) as { pages: string[] }
  const navPages = meta.pages.filter((page) => !page.startsWith('---'))
  const pages = pagesFor(locale)
  invariant(navPages.length === pages.size, `${locale}/meta.json does not list every page exactly once`)
  for (const page of navPages) {
    invariant(pages.has(page), `${locale}/meta.json references missing page: ${page}`)
  }
}

const internalLinkPattern = /(?:\]\(|href=")\/(en|zh)\/docs\/([^\s)#"]+)(?:#([^\s)"]+))?/g
// A link to a heading on its own page.
const samePageLinkPattern = /(?:\]\(|href=")#([^\s)"]+)/g
for (const locale of locales) {
  for (const page of pagesFor(locale)) {
    const source = await text(join(contentRoot, locale, `${page}.mdx`))
    for (const match of source.matchAll(internalLinkPattern)) {
      const [, targetLocale, targetPage, encodedFragment] = match
      invariant(
        localePages.get(targetLocale)?.has(targetPage),
        `${locale}/${page} links to missing ${targetLocale}/${targetPage}`
      )
      if (encodedFragment) {
        const target = await text(join(contentRoot, targetLocale, `${targetPage}.mdx`))
        // A fragment names an id exactly, case included.
        const fragment = decodeURIComponent(encodedFragment)
        invariant(
          (await headingIds(target)).has(fragment),
          `${locale}/${page} links to missing heading ${targetLocale}/${targetPage}#${fragment}`
        )
      }
    }
    const ids = await headingIds(source)
    for (const [, encodedFragment] of source.matchAll(samePageLinkPattern)) {
      const fragment = decodeURIComponent(encodedFragment)
      invariant(ids.has(fragment), `${locale}/${page} links to missing heading #${fragment}`)
    }
  }
}

const cargo = await text(join(repository, 'Cargo.toml'))
const license = cargo.match(/^license\s*=\s*"([^"]+)"/m)?.[1]
const packageVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1]
invariant(license === 'AGPL-3.0-or-later', `Unexpected workspace license: ${license ?? 'missing'}`)
invariant(packageVersion, 'Unable to read the workspace package version')

const landing = await text(join(docsApp, 'src/components/landing/translations.ts'))
invariant(!/\bMIT\b/.test(landing), 'Landing page still claims an MIT license')
invariant(landing.includes(license), 'Landing page does not show the workspace license')
const landingVersion = landing.match(/export const LANDING_VERSION = '([^']+)'/)?.[1]
invariant(
  landingVersion === packageVersion,
  `LANDING_VERSION is ${landingVersion ?? 'missing'}, expected ${packageVersion}: ` +
    'update it in apps/docs/src/components/landing/translations.ts'
)

// Every documentation page the landing can link to exists in both locales, and every heading it links to exists.
for (const page of docsPages) {
  for (const locale of locales) {
    invariant(
      pagesFor(locale).has(page),
      `The landing links to missing ${locale}/${page}: update docsPages in ${landingTranslations}`
    )
  }
}
const landingParts = collectLandingCopy(landingCopy, { links: [], strings: [] })
invariant(
  landingParts.links.length > 0,
  'Found no documentation links in the landing copy: update collectLandingCopy in apps/docs/scripts/check-contracts.ts'
)
for (const link of landingParts.links) {
  invariant(
    localePages.get(link.lang)?.has(link.page),
    `The landing links to missing ${link.lang}/${link.page}: update ${landingTranslations}`
  )
  if (link.hash) {
    const target = await text(join(contentRoot, link.lang, `${link.page}.mdx`))
    invariant(
      (await headingIds(target)).has(link.hash),
      `The landing links to missing heading ${link.lang}/${link.page}#${link.hash}: update ${landingTranslations}`
    )
  }
}

// The landing's theme toggle (useTheme in landing/chrome/header.tsx) reaches the ThemeProvider inside fumadocs'
// RootProvider only while both load the same next-themes module.
const fumadocsUi = await packageDirectory('fumadocs-ui', docsApp)
invariant(fumadocsUi, 'apps/docs cannot resolve fumadocs-ui')
const appNextThemes = await packageDirectory('next-themes', docsApp)
const fumadocsNextThemes = await packageDirectory('next-themes', fumadocsUi)
invariant(
  appNextThemes !== undefined && appNextThemes === fumadocsNextThemes,
  `apps/docs resolves next-themes to ${appNextThemes ?? 'nothing'}, but fumadocs-ui resolves it to ` +
    `${fumadocsNextThemes ?? 'nothing'}: pin next-themes in apps/docs/package.json to the version fumadocs-ui uses, ` +
    'so the landing theme toggle and the docs share one theme'
)

// Every dark rule is written twice: for the .dark class next-themes sets, and as the prefers-color-scheme fallback
// that applies before JavaScript runs. The two must declare the same values.
for (const file of ['landing.css', 'landing-mocks.css']) {
  const stylesheet = `apps/docs/src/styles/${file}`
  const blocks = cssBlocks(await text(join(docsApp, 'src/styles', file)))
  const classRules = new Map<string, CssBlock>()
  const fallbackRules = new Map<string, CssBlock>()
  for (const block of blocks) {
    if (block.prelude.startsWith(':root.dark')) {
      classRules.set(block.prelude.replace(darkClassRoot, ':root'), block)
    }
    if (darkMediaQuery.test(block.prelude)) {
      for (const rule of block.children) {
        if (rule.prelude.startsWith(darkMediaRoot)) {
          fallbackRules.set(rule.prelude.replaceAll(darkMediaRoot, ':root'), rule)
        }
      }
    }
  }
  invariant(
    classRules.size > 0,
    `${stylesheet} has no :root.dark rules: update the dark palette check in apps/docs/scripts/check-contracts.ts`
  )
  for (const [key, rule] of classRules) {
    const fallback = fallbackRules.get(key)
    invariant(fallback, `${stylesheet}: "${rule.prelude}" has no prefers-color-scheme: dark fallback`)
    const declared = cssDeclarations(rule.body)
    const fallbackDeclared = cssDeclarations(fallback.body)
    const drift = [...new Set([...declared.keys(), ...fallbackDeclared.keys()])].find(
      (property) => declared.get(property) !== fallbackDeclared.get(property)
    )
    invariant(
      drift === undefined,
      `${stylesheet}: ${drift} differs between "${rule.prelude}" and its prefers-color-scheme: dark fallback ` +
        `"${fallback.prelude}"; keep both dark palettes identical`
    )
  }
  for (const [key, fallback] of fallbackRules) {
    invariant(classRules.has(key), `${stylesheet}: "${fallback.prelude}" has no :root.dark rule with the same values`)
  }
}

// fumadocs' light theme put muted text and the focus ring under the contrast they need on its surfaces, 4.5:1 for text
// and 3:1 for a focus indicator, and src/styles/app.css raises them. Every theme keeps them, with the colors that the
// rules of app.css and of the stylesheets it imports set, and a rule that sets them for a theme this check does not
// know fails it.
const surfaces = ['background', 'card', 'secondary', 'muted', 'popover']
const foregrounds = [
  ['muted-foreground', 4.5],
  ['ring', 3]
] as const
const checkedColors = new Set(
  [...surfaces, ...foregrounds.map(([token]) => token)].map((token) => `--color-fd-${token}`)
)
// The rules that set them, weakest first, and the themes each sets them for. `@theme` colors are in a cascade layer,
// under every other rule, and `.dark #nd-sidebar` sets the colors of the sidebar.
const themeRules: [prelude: string, themes: string[]][] = [
  ['@theme', ['light', 'dark', 'dark sidebar']],
  ['.dark', ['dark', 'dark sidebar']],
  [':root.dark', ['dark', 'dark sidebar']],
  [':root:not(.dark)', ['light']],
  ['.dark #nd-sidebar', ['dark sidebar']]
]
const colorRules: { colors: Map<string, string>; prelude: string }[] = []
for (const { blocks, file } of await stylesheetBlocks(join(docsApp, 'src/styles/app.css'))) {
  for (const [block, outer] of nestedBlocks(blocks)) {
    const colors = new Map([...cssDeclarations(block.ownBody)].filter(([property]) => checkedColors.has(property)))
    if (colors.size > 0) {
      invariant(
        outer.length === 0 && themeRules.some(([prelude]) => prelude === block.prelude),
        `${relative(repository, file)}: "${[...outer, block.prelude].join(' { ')}" sets ${[...colors.keys()].join(', ')} ` +
          'for a theme the contrast check does not know: update apps/docs/scripts/check-contracts.ts'
      )
      colorRules.push({ colors, prelude: block.prelude })
    }
  }
}
const themes = new Map<string, Map<string, string>>()
for (const [prelude, names] of themeRules) {
  for (const rule of colorRules.filter((candidate) => candidate.prelude === prelude)) {
    for (const name of names) {
      themes.set(name, new Map([...(themes.get(name) ?? []), ...rule.colors]))
    }
  }
}
for (const theme of new Set(themeRules.flatMap(([, names]) => names))) {
  const color = (token: string) => {
    const value = themes.get(theme)?.get(`--color-fd-${token}`)
    invariant(
      value,
      `No rule sets the ${theme} --color-fd-${token}: update the contrast check in apps/docs/scripts/check-contracts.ts`
    )
    return hslColor(value)
  }
  for (const surface of surfaces) {
    const ground = color(surface)
    for (const [token, minimum] of foregrounds) {
      const ratio = contrastRatio(composite(color(token), ground), ground)
      invariant(
        ratio >= minimum,
        `The ${theme} --color-fd-${token} is ${ratio.toFixed(2)}:1 on --color-fd-${surface}, under ${minimum}:1: ` +
          'update apps/docs/src/styles/app.css'
      )
    }
  }
}

// fumadocs-ui falls back to its English text for a key a language leaves out, and some of that text appears only after
// a click, such as the names of the sidebar triggers once the sidebar is open or collapsed, where no route check sees
// it. Chinese translates every key, the keys patches/fumadocs-ui@16.6.16.patch adds among them.
for (const [key, english] of Object.entries(defaultTranslations)) {
  const chinese = uiTranslations.zh[key as keyof Translations]
  invariant(
    chinese && /\p{Script=Han}/u.test(chinese),
    `apps/docs/src/lib/ui-translations.ts leaves fumadocs-ui's "${english}" (${key}) untranslated in zh`
  )
}
// Chinese translates the search dialog's own text too, which appears only once the dialog is open.
for (const [key, value] of Object.entries(searchDialogText.zh)) {
  const chinese = typeof value === 'function' ? value(2) : value
  invariant(
    /\p{Script=Han}/u.test(chinese),
    `apps/docs/src/lib/ui-translations.ts leaves the search dialog's ${key} untranslated in zh`
  )
}

const constants = await text(join(repository, 'crates/common/src/constants.rs'))
const protocolVersion = constants.match(/PROTOCOL_VERSION:\s*u32\s*=\s*(\d+)/)?.[1]
invariant(protocolVersion, 'Unable to read PROTOCOL_VERSION')
for (const locale of locales) {
  const architecture = await text(join(contentRoot, locale, 'architecture.mdx'))
  invariant(architecture.includes(`\`${protocolVersion}\``), `${locale}/architecture.mdx has a stale protocol version`)
}

const installer = await text(join(repository, 'deploy/install.sh'))
invariant(
  installer.includes('DOCS_URL="https://docs.serverbee.app"'),
  'Installer does not emit the canonical documentation host'
)
invariant(
  /RELEASE_CHANNEL="\$\{SERVERBEE_CHANNEL:-auto\}"/.test(installer),
  'Installer does not use the maintainable auto release policy by default'
)
invariant(/"channel": "\$\{RELEASE_CHANNEL\}"/.test(installer), 'Installer does not persist the release policy')
invariant(
  installer.includes('/server.toml:/etc/serverbee/server.toml:ro'),
  'Installer Docker config is not mounted at a supported path'
)
invariant(installer.includes('http://127.0.0.1:9527/healthz'), 'Installer Docker health check is not IPv4-explicit')

for (const commandSource of [
  'apps/web/src/components/server/add-server-dialog.tsx',
  'apps/web/src/components/server/agent-reenrollment-dialog.tsx',
  'apps/ios/ServerBee/ViewModels/AgentLifecycleViewModel.swift'
]) {
  const source = await text(join(repository, commandSource))
  invariant(!source.includes('--channel '), `${commandSource} hardcodes a release channel that can become stale`)
}

const allDocumentation = (
  await Promise.all(
    locales.flatMap((locale) => [...pagesFor(locale)].map((page) => text(join(contentRoot, locale, `${page}.mdx`))))
  )
).join('\n')

const envReference = await text(join(repository, 'ENV.md'))
const referencedEnvVars = new Set([...envReference.matchAll(/`(SERVERBEE_[A-Z0-9_]+)`/g)].map((match) => match[1]))
for (const locale of locales) {
  const configuration = await text(join(contentRoot, locale, 'configuration.mdx'))
  const documentedEnvVars = new Set([...configuration.matchAll(/`(SERVERBEE_[A-Z0-9_]+)`/g)].map((match) => match[1]))
  const missingEnvVars = [...referencedEnvVars].filter((variable) => !documentedEnvVars.has(variable))
  invariant(missingEnvVars.length === 0, `${locale}/configuration.mdx omits env vars: ${missingEnvVars.join(', ')}`)
}

invariant(
  !/ghcr\.io\/zingerlittlebee\/serverbee-(?:server|agent):latest/.test(allDocumentation),
  'User documentation still deploys the potentially stale GHCR :latest tag'
)
for (const match of allDocumentation.matchAll(/ghcr\.io\/zingerlittlebee\/serverbee-(?:server|agent):([^\s`"']+)/g)) {
  invariant(match[1] === packageVersion, `Documentation image tag ${match[1]} differs from package ${packageVersion}`)
}
for (const match of allDocumentation.matchAll(/releases\/download\/v([^/]+)\/serverbee-(?:server|agent)-/g)) {
  invariant(match[1] === packageVersion, `Documentation download ${match[1]} differs from package ${packageVersion}`)
}

const railwayDockerfile = await text(join(repository, 'deploy/railway/Dockerfile'))
invariant(
  railwayDockerfile.includes(`ARG SERVERBEE_IMAGE_TAG=${packageVersion}`),
  'Railway image default differs from the workspace package version'
)
const releaseWorkflow = await text(join(repository, '.github/workflows/release.yml'))
invariant(releaseWorkflow.includes('echo "$IMAGE:beta"'), 'Release workflow does not move the prerelease beta tag')
invariant(releaseWorkflow.includes('echo "$IMAGE:latest"'), 'Release workflow does not move the stable latest tag')

const iosProject = await text(join(repository, 'apps/ios/project.yml'))
const iosTarget = iosProject.match(/iOS:\s*"([^"]+)"/)?.[1]
invariant(iosTarget, 'Unable to read the iOS deployment target')
for (const locale of locales) {
  const mobile = await text(join(contentRoot, locale, 'mobile.mdx'))
  invariant(mobile.includes(`iOS ${iosTarget}`), `${locale}/mobile.mdx has a stale iOS deployment target`)
}
const iosMajor = iosTarget.split('.')[0]
for (const copyText of landingParts.strings) {
  for (const match of copyText.matchAll(iosVersionMention)) {
    invariant(
      match[1] === iosMajor,
      `The landing copy names iOS ${match[1]}, but apps/ios/project.yml targets iOS ${iosTarget}: ` +
        `update ${landingTranslations}`
    )
  }
}

const [serverConfig, brandRouter, geoipService, asnService, settingsRouter] = await Promise.all(
  [
    'crates/server/src/config.rs',
    'crates/server/src/router/api/brand.rs',
    'crates/server/src/service/geoip.rs',
    'crates/server/src/service/asn.rs',
    'crates/server/src/router/api/setting.rs'
  ].map((path) => text(join(repository, path)))
)
const databaseFilename = serverConfig.match(/fn default_db_path\(\)[^{]*\{\s*"([^"]+)"/)?.[1]
const brandDirectory = brandRouter.match(/fn brand_dir\([\s\S]*?\.join\("([^"]+)"\)/)?.[1]
const countryFilename = geoipService.match(/pub const DBIP_FILENAME[^=]*=\s*"([^"]+)"/)?.[1]
const asnFilename = asnService.match(/pub const DBIP_ASN_FILENAME[^=]*=\s*"([^"]+)"/)?.[1]
invariant(
  databaseFilename && brandDirectory && countryFilename && asnFilename,
  'Unable to resolve persistent asset paths'
)
const persistentAssets = [brandDirectory, countryFilename, asnFilename]
const backupHandler = settingsRouter.split('pub async fn create_backup(')[1]?.split('pub async fn restore_backup(')[0]
invariant(
  backupHandler?.includes('VACUUM INTO') && backupHandler.includes('tokio::fs::read(&backup_path)'),
  'The backup export implementation changed; review the documented database-only scope'
)
const linuxReleaseTargets = [...releaseWorkflow.matchAll(/target:\s*(\S+-unknown-linux-\S+)/g)].map((match) => match[1])
invariant(
  linuxReleaseTargets.length > 0 && linuxReleaseTargets.every((target) => target.endsWith('-musl')),
  'Linux release linkage changed; review the allocator guidance'
)

for (const locale of locales) {
  const deployment = await text(join(contentRoot, locale, 'deployment.mdx'))
  invariant(
    deployment.includes('./config/server.toml:/etc/serverbee/server.toml:ro'),
    `${locale}/deployment.mdx uses an unsupported Docker config path`
  )
  invariant(
    deployment.includes('http://127.0.0.1:9527/healthz'),
    `${locale}/deployment.mdx uses a fragile Docker health URL`
  )
  const inventoryAnchor = deployment.indexOf('[#persistent-data]')
  invariant(inventoryAnchor >= 0, `${locale}/deployment.mdx has no persistent-data inventory`)
  const inventory = firstMarkdownTable(deployment.slice(inventoryAnchor))
  const separateAssetLabel = locale === 'en' ? 'No' : '否'
  for (const asset of persistentAssets) {
    const path = `{data_dir}/${asset}${asset === brandDirectory ? '/' : ''}`
    const row = inventory.find((cells) => cells[1]?.includes(`\`${path}\``))
    invariant(row?.[2] === separateAssetLabel, `${locale}/deployment.mdx omits the separate persistent asset ${path}`)
  }
  for (const configPath of ['geoip.mmdb_path', 'asn.mmdb_path', '/opt/serverbee/etc/']) {
    invariant(
      inventory.some((cells) => cells[1]?.includes(configPath) && cells[2] === separateAssetLabel),
      `${locale}/deployment.mdx omits the separate configuration path ${configPath}`
    )
  }
  const databaseRow = inventory.find((cells) => cells[1]?.includes(`{data_dir}/${databaseFilename}`))
  invariant(
    databaseRow?.[2] === (locale === 'en' ? 'Yes' : '是'),
    `${locale}/deployment.mdx misstates database backup scope`
  )
  const prose = normalizedProse(deployment)
  const exportScope = locale === 'en' ? /export\w* only (?:the )?database/i : /仅导出数据库/
  const restoreScope = locale === 'en' ? /restore\w* only (?:the )?database/i : /仅恢复数据库/
  invariant(
    prose.includes('/api/settings/backup') && exportScope.test(prose),
    `${locale}/deployment.mdx does not explain the database-only backup API`
  )
  invariant(
    prose.includes('/api/settings/restore') && restoreScope.test(prose),
    `${locale}/deployment.mdx does not explain the database-only restore API`
  )
  const shellExamples = [...deployment.matchAll(shellCodeBlock)].map((match) => match[1])
  invariant(
    shellExamples.some(
      (code) =>
        code.includes('/assets.tar.gz') &&
        code.includes(`--exclude='./${databaseFilename}*'`) &&
        code.includes('-C /opt/serverbee/data .') &&
        code.includes('/config.tar.gz')
    ),
    `${locale}/deployment.mdx has no asset/configuration archive paired with its database backup`
  )
  invariant(
    shellExamples.some(
      (code) =>
        code.includes('tar xzf "$SERVERBEE_RESTORE_DIR/assets.tar.gz"') &&
        code.includes('tar xzf "$SERVERBEE_RESTORE_DIR/config.tar.gz"')
    ),
    `${locale}/deployment.mdx does not restore separate assets and configuration`
  )
  invariant(
    !allocatorAssignment.test(deployment),
    `${locale}/deployment.mdx sets a glibc allocator option in the standard musl deployment`
  )
  const resourceUsage = normalizedProse(await text(join(contentRoot, locale, 'resource-usage.mdx')))
  const muslScope = locale === 'en' ? /musl.{0,100}(?:no effect|does not affect)/i : /musl.{0,60}(?:无效|不生效)/i
  const historicalScope = locale === 'en' ? /historical/i : /历史/
  invariant(
    muslScope.test(resourceUsage) && resourceUsage.includes('glibc') && resourceUsage.includes('MALLOC_ARENA_MAX'),
    `${locale}/resource-usage.mdx does not distinguish allocator/build scope`
  )
  invariant(
    historicalScope.test(resourceUsage) && resourceUsage.includes('v0.9.3'),
    `${locale}/resource-usage.mdx presents legacy observations without their historical version scope`
  )
}

const enTesting = await text(join(contentRoot, 'en/testing.mdx'))
const zhTesting = await text(join(contentRoot, 'zh/testing.mdx'))
for (const [locale, testing] of [
  ['en', enTesting],
  ['zh', zhTesting]
] as const) {
  const overview = firstMarkdownTable(testing)
  invariant(overview.length > 1, `${locale}/testing.mdx does not have a test overview table`)
  invariant(
    overview.every((row) => row.length === 2),
    `${locale}/testing.mdx test overview must not contain a per-area census column`
  )
  invariant(
    !/^(?:tests?|test count|测试数|测试数量)$/i.test(overview[0][1] ?? ''),
    `${locale}/testing.mdx test overview must describe coverage instead of snapshotting counts`
  )
  invariant(
    overview.slice(1).every((row) => !numericTableCell.test(row[1] ?? '')),
    `${locale}/testing.mdx test overview contains a per-area census`
  )
}

const enTestingProse = normalizedProse(enTesting)
for (const fact of [
  /runs? only the gates for the areas/i,
  /widget.{0,80}run the rust gates/i,
  /(?:workflow file|usable base commit).{0,120}runs? every gate/i
]) {
  invariant(fact.test(enTestingProse), 'en/testing.mdx does not explain the path-aware CI areas')
}

const zhTestingProse = normalizedProse(zhTesting)
for (const fact of [
  /只运行受影响领域的检查/,
  /widget.{0,60}也会运行 Rust 检查/i,
  /(?:工作流文件|基准提交).{0,80}都会运行全部检查/
]) {
  invariant(fact.test(zhTestingProse), 'zh/testing.mdx does not explain the path-aware CI areas')
}

const ciWorkflow = await text(join(repository, '.github/workflows/ci.yml'))
for (const area of ['rust', 'web', 'lint', 'install']) {
  invariant(
    new RegExp(`area ${area} '`).test(ciWorkflow) && ciWorkflow.includes(`needs.changes.outputs.${area} == 'true'`),
    `CI no longer gates jobs on the ${area} area; update the testing documentation`
  )
}
invariant(
  /area rust '[^\n]*builtin-widgets/.test(ciWorkflow),
  'CI no longer runs Rust gates for built-in widget changes; update the testing documentation'
)
invariant(
  /\^\\\.github\/workflows\/[\s\S]{0,120}run_all=1/.test(ciWorkflow),
  'CI no longer runs every gate for workflow changes; update the testing documentation'
)

const enIndex = await text(join(contentRoot, 'en/index.mdx'))
const zhIndex = await text(join(contentRoot, 'zh/index.mdx'))
invariant(!enIndex.includes('every change passes'), 'en/index.mdx overstates CI coverage')
invariant(!zhIndex.includes('每次改动都要通过'), 'zh/index.mdx overstates CI coverage')
const enIndexProse = normalizedProse(enIndex)
for (const fact of [
  /changed files/i,
  /only for the areas/i,
  /(?:run|enable).{0,60}rust (?:(?:test )?jobs|gates|checks|tests)|rust (?:(?:test )?jobs|gates|checks|tests).{0,60}(?:run|enabled)/i
]) {
  invariant(fact.test(enIndexProse), 'en/index.mdx does not describe the path-aware CI areas')
}

const zhIndexProse = normalizedProse(zhIndex)
for (const fact of [/变更文件/, /只针对变更涉及的领域/, /(?:运行|执行).{0,40}Rust (?:任务|质量门槛|检查|测试)/i]) {
  invariant(fact.test(zhIndexProse), 'zh/index.mdx does not describe the path-aware CI areas')
}

const zhAlerts = await text(join(contentRoot, 'zh/alerts.mdx'))
const zhAlertsProse = normalizedProse(zhAlerts)
invariant(
  /Agent.{0,100}(?:上报|拥有|具备).{0,80}(?:firewall_block|CAP_FIREWALL_BLOCK)/i.test(zhAlertsProse),
  'zh/alerts.mdx does not identify the agent-owned firewall capability'
)
invariant(
  /(?:Server|服务端).{0,60}(?:不能|无法|不可).{0,40}(?:切换|修改|配置|授予)/i.test(zhAlertsProse),
  'zh/alerts.mdx implies that the server can change agent capabilities'
)
invariant(
  !/(?:服务器|Server).{0,30}(?:具备|拥有).{0,40}CAP_FIREWALL_BLOCK/i.test(zhAlertsProse),
  'zh/alerts.mdx assigns CAP_FIREWALL_BLOCK ownership to the server'
)

console.log('PASS: documentation contracts')
