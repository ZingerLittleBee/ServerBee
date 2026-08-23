import { readdir, readFile } from 'node:fs/promises'
import { basename, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const docsApp = resolve(fileURLToPath(new URL('..', import.meta.url)))
const repository = resolve(docsApp, '../..')
const contentRoot = join(docsApp, 'content/docs')
const locales = ['en', 'zh'] as const

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(message)
  }
}

function text(path: string): Promise<string> {
  return readFile(path, 'utf8')
}

function headingSlugs(markdown: string): Set<string> {
  const counts = new Map<string, number>()
  const slugs = new Set<string>()
  for (const match of markdown.matchAll(/^#{1,6}\s+(.+)$/gm)) {
    const heading = match[1]
      .replace(/<[^>]+>/g, '')
      .replace(/[`*_~]/g, '')
      .trim()
      .toLowerCase()
    const base = heading
      .replace(/[^\p{L}\p{N}\s-]/gu, '')
      .replace(/\s+/g, '-')
      .replace(/-+/g, '-')
    const count = counts.get(base) ?? 0
    counts.set(base, count + 1)
    slugs.add(count === 0 ? base : `${base}-${count}`)
  }
  return slugs
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
        const fragment = decodeURIComponent(encodedFragment).toLowerCase()
        invariant(
          headingSlugs(target).has(fragment),
          `${locale}/${page} links to missing heading ${targetLocale}/${targetPage}#${fragment}`
        )
      }
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
}

console.log('PASS: documentation contracts')
