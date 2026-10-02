import { open, readFile, stat } from 'node:fs/promises'
import { dirname, isAbsolute, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const NAVIGATION_DOCUMENTS = [
  'AGENTS.md',
  '.claude/commands/release-docs.md',
  'docs/agents/navigation.md',
  'docs/agents/domain.md',
  'apps/docs/README.md',
  'tests/README.md',
  'tests/network-quality.md'
] as const

const HISTORICAL_DOCUMENTS = [
  'docs/superpowers/specs/2026-03-12-serverbee-architecture-design.md',
  'docs/superpowers/plans/PROGRESS.md'
] as const

const HISTORICAL_MARKER = '<!-- agent-navigation: historical -->'
const MARKDOWN_LINK = /\[[^\]]*\]\((?:<([^>]+)>|([^\s)]+))(?:\s+["'][^"']*["'])?\)/g
const INLINE_CODE = /`([^`\n]+)`/g
const EXTERNAL_TARGET = /^(?:[a-z][a-z\d+.-]*:|\/\/|#)/i
const INLINE_SOURCE_PATH = /^(?:apps|crates|docs|packages|tests|\.github|\.claude|scripts|src|content|public)\/[^\s]+$/
const ROOT_SOURCE_PATH = /^(?:apps|crates|docs|packages|tests|\.github|\.claude)\//
const OLD_DOCS_LOCALE = /(?:apps\/docs\/)?content\/docs\/(?:cn(?:\/|\b)|\{[^}]*\bcn\b)/
const BRACE_OPTIONS = /\{([^{}]+)\}/
const URL_SUFFIX = /[?#]/
const NON_LITERAL_PATH = /[*<>]/

interface NavigationCheckOptions {
  documents?: readonly string[]
  historicalDocuments?: readonly string[]
}

function expandPaths(path: string): string[] {
  const match = path.match(BRACE_OPTIONS)
  if (!match) {
    return [path]
  }
  return match[1].split(',').flatMap((option) => expandPaths(path.replace(match[0], option)))
}

function localTargets(source: string): string[] {
  const links = [...source.matchAll(MARKDOWN_LINK)].map((match) => match[1] ?? match[2])
  return [...new Set(links.filter((target) => !EXTERNAL_TARGET.test(target)))]
}

async function validateTarget(
  repository: string,
  file: string,
  target: string,
  rootRelative = false
): Promise<string[]> {
  let path: string
  try {
    path = decodeURIComponent(target.split(URL_SUFFIX)[0])
  } catch {
    return [`${file}: invalid escaped path: ${target}`]
  }
  if (!path) {
    return []
  }
  if (isAbsolute(path)) {
    return [`${file}: use a portable relative repository link: ${target}`]
  }

  const base = rootRelative ? repository : dirname(resolve(repository, file))
  return (
    await Promise.all(
      expandPaths(path).map(async (expanded) => {
        const absolute = resolve(base, expanded)
        const fromRoot = relative(repository, absolute)
        if (fromRoot === '..' || fromRoot.startsWith('../')) {
          return `${file}: link escapes the repository: ${target}`
        }
        try {
          await stat(absolute)
          return undefined
        } catch {
          return `${file}: missing navigation target: ${expanded}`
        }
      })
    )
  ).filter((issue): issue is string => issue !== undefined)
}

async function validateDocument(repository: string, file: string): Promise<string[]> {
  let source: string
  try {
    source = await readFile(resolve(repository, file), 'utf8')
  } catch {
    return [`${file}: navigation document is missing or unreadable`]
  }

  const issues: string[] = []
  if (OLD_DOCS_LOCALE.test(source)) {
    issues.push(`${file}: unsupported docs locale cn; use zh`)
  }
  issues.push(
    ...(await Promise.all(localTargets(source).map((target) => validateTarget(repository, file, target)))).flat()
  )

  // Source pointers in inline code are common in app READMEs; generated outputs are not navigation targets.
  const inlinePaths = [...new Set([...source.matchAll(INLINE_CODE)].map((match) => match[1]))].filter(
    (path) => INLINE_SOURCE_PATH.test(path) && !NON_LITERAL_PATH.test(path)
  )
  issues.push(
    ...(
      await Promise.all(
        inlinePaths.map((path) =>
          validateTarget(
            repository,
            file,
            path,
            ROOT_SOURCE_PATH.test(path) || (path.startsWith('scripts/') && !file.startsWith('apps/'))
          )
        )
      )
    ).flat()
  )
  return issues
}

async function validateHistory(repository: string, file: string): Promise<string[]> {
  try {
    const handle = await open(resolve(repository, file), 'r')
    try {
      const buffer = Buffer.alloc(4096)
      const { bytesRead } = await handle.read(buffer, 0, buffer.length, 0)
      const header = buffer.subarray(0, bytesRead).toString('utf8').split('\n').slice(0, 20).join('\n')
      return header.includes(HISTORICAL_MARKER) ? [] : [`${file}: add the historical marker near the document title`]
    } finally {
      await handle.close()
    }
  } catch {
    return [`${file}: historical document is missing or unreadable`]
  }
}

export async function checkAgentNavigation(
  repository: string,
  { documents = NAVIGATION_DOCUMENTS, historicalDocuments = HISTORICAL_DOCUMENTS }: NavigationCheckOptions = {}
): Promise<string[]> {
  const root = resolve(repository)
  const results = await Promise.all([
    ...documents.map((file) => validateDocument(root, file)),
    ...historicalDocuments.map((file) => validateHistory(root, file))
  ])
  return [...new Set(results.flat())].sort()
}

if (import.meta.main) {
  const repository = fileURLToPath(new URL('..', import.meta.url))
  const issues = await checkAgentNavigation(repository)
  if (issues.length > 0) {
    for (const issue of issues) {
      console.error(issue)
    }
    process.exitCode = 1
  } else {
    console.log('Agent navigation paths and historical markers are valid.')
  }
}
