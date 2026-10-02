import type { Dirent } from 'node:fs'
import { readdir, readFile, stat } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { TOML, YAML } from 'bun'

const rustSourceExtension = /\.rs$/
const whitespace = /\s+/

function record(value: unknown, label: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${label} must be an object`)
  }
  return value
}

async function exists(path: string): Promise<boolean> {
  try {
    return (await stat(path)).isFile()
  } catch (error) {
    if (error instanceof Error && 'code' in error && error.code === 'ENOENT') {
      return false
    }
    throw error
  }
}

async function directoryEntries(path: string): Promise<Dirent[]> {
  try {
    return await readdir(path, { withFileTypes: true })
  } catch (error) {
    if (error instanceof Error && 'code' in error && error.code === 'ENOENT') {
      return []
    }
    throw error
  }
}

async function automaticIntegrationTargets(directory: string, explicitPaths: Set<string>): Promise<string[]> {
  const targets: string[] = []
  for (const entry of await directoryEntries(directory)) {
    const file = join(directory, entry.name)
    const source = entry.isSymbolicLink() ? await stat(file) : entry
    if (source.isFile() && entry.name.endsWith('.rs') && !explicitPaths.has(resolve(file))) {
      targets.push(entry.name.replace(rustSourceExtension, ''))
    } else if (source.isDirectory()) {
      const main = join(file, 'main.rs')
      if ((await exists(main)) && !explicitPaths.has(resolve(main))) {
        targets.push(entry.name)
      }
    }
  }
  return targets
}

async function packageEdition(crate: string, packageConfig: Record<string, unknown>): Promise<unknown> {
  if (packageConfig.edition === undefined) {
    return '2015'
  }
  if (typeof packageConfig.edition === 'string') {
    return packageConfig.edition
  }
  const inherited = record(packageConfig.edition, 'Cargo edition')
  if (inherited.workspace !== true) {
    throw new Error('Cargo edition must be a string or inherited from the workspace')
  }
  let directory = typeof packageConfig.workspace === 'string' ? resolve(crate, packageConfig.workspace) : crate
  while (true) {
    const path = join(directory, 'Cargo.toml')
    if (await exists(path)) {
      const manifest = record(TOML.parse(await readFile(path, 'utf8')), 'Workspace manifest')
      if (manifest.workspace !== undefined) {
        const workspace = record(manifest.workspace, 'Cargo workspace')
        return record(workspace.package, 'Workspace package defaults').edition
      }
    }
    const parent = dirname(directory)
    if (parent === directory || typeof packageConfig.workspace === 'string') {
      throw new Error('Cannot resolve the inherited Cargo edition')
    }
    directory = parent
  }
}

async function discoversTests(crate: string, manifest: Record<string, unknown>): Promise<boolean> {
  const packageConfig = record(manifest.package, 'Cargo package')
  const edition = await packageEdition(crate, packageConfig)
  if (typeof edition !== 'string' || !['2015', '2018', '2021', '2024'].includes(edition)) {
    throw new Error(`Unsupported Cargo edition: ${String(edition)}`)
  }
  if (packageConfig.autotests !== undefined && typeof packageConfig.autotests !== 'boolean') {
    throw new Error('Cargo autotests must be a boolean')
  }
  if (typeof packageConfig.autotests === 'boolean') {
    return packageConfig.autotests
  }
  // Cargo 2015 disables automatic discovery when any target is manually declared.
  const hasExplicitTargets = ['lib', 'bin', 'example', 'test', 'bench'].some((key) => manifest[key] !== undefined)
  return edition !== '2015' || !hasExplicitTargets
}

/** Follow Cargo's automatic test discovery, including tests/<name>/main.rs and explicit [[test]] names. */
export async function integrationTargets(crate: string): Promise<string[]> {
  const manifest = record(TOML.parse(await readFile(join(crate, 'Cargo.toml'), 'utf8')), 'Cargo manifest')
  const targets = new Set<string>()
  const explicitPaths = new Set<string>()
  if (manifest.test !== undefined) {
    if (!Array.isArray(manifest.test)) {
      throw new Error('Cargo test entries must be an array')
    }
    for (const entry of manifest.test) {
      const test = record(entry, 'Cargo test entry')
      if (typeof test.name !== 'string' || test.name.length === 0) {
        throw new Error('Cargo test entry must have a name')
      }
      targets.add(test.name)
      if (typeof test.path === 'string') {
        explicitPaths.add(resolve(crate, test.path))
      }
    }
  }
  if (await discoversTests(crate, manifest)) {
    for (const name of await automaticIntegrationTargets(join(crate, 'tests'), explicitPaths)) {
      targets.add(name)
    }
  }
  return [...targets].sort()
}

function matrixSelectors(workflow: string): string[] {
  const document = record(YAML.parse(workflow), 'CI workflow')
  const jobs = record(document.jobs, 'CI jobs')
  const job = record(jobs['rust-tests-server'], 'rust-tests-server job')
  const strategy = record(job.strategy, 'Server test strategy')
  const matrix = record(strategy.matrix, 'Server test matrix')
  if (!Array.isArray(matrix.include) || matrix.include.length === 0) {
    throw new Error('Server test matrix must have at least one include group')
  }

  const selectors: string[] = []
  for (const entry of matrix.include) {
    const group = record(entry, 'Server test group')
    if (typeof group.tests !== 'string' || group.tests.trim().length === 0) {
      throw new Error(`Server test group ${String(group.group)} must have test selectors`)
    }
    const args = group.tests.trim().split(whitespace)
    for (let index = 0; index < args.length; index += 1) {
      const argument = args[index]
      const next = args[index + 1]
      if (argument === '--test' && next && !next.startsWith('-')) {
        selectors.push(next)
        index += 1
      } else if (argument?.startsWith('--test=') && argument.length > '--test='.length) {
        selectors.push(argument.slice('--test='.length))
      } else {
        throw new Error(`Invalid server test selector in group ${String(group.group)}: ${String(argument)}`)
      }
    }
  }
  return selectors
}

/** Every integration target must occur exactly once; stale and duplicate selectors also fail the check. */
export function validateIntegrationTargets(targets: string[], workflow: string): void {
  const available = new Set(targets)
  const counts = new Map<string, number>()
  for (const selector of matrixSelectors(workflow)) {
    counts.set(selector, (counts.get(selector) ?? 0) + 1)
  }
  const missing = [...available].filter((target) => !counts.has(target)).sort()
  const duplicate = [...counts]
    .filter(([, count]) => count > 1)
    .map(([target]) => target)
    .sort()
  const stale = [...counts.keys()].filter((target) => !available.has(target)).sort()
  const errors = [
    ...(missing.length ? [`Missing integration targets: ${missing.join(', ')}`] : []),
    ...(duplicate.length ? [`Duplicate integration selectors: ${duplicate.join(', ')}`] : []),
    ...(stale.length ? [`Unknown integration selectors: ${stale.join(', ')}`] : [])
  ]
  if (errors.length > 0) {
    throw new Error(errors.join('\n'))
  }
}

export async function checkIntegrationTargets(repository: string): Promise<number> {
  const targets = await integrationTargets(join(repository, 'crates/server'))
  validateIntegrationTargets(targets, await readFile(join(repository, '.github/workflows/ci.yml'), 'utf8'))
  return targets.length
}

if (import.meta.main) {
  try {
    const count = await checkIntegrationTargets(resolve(import.meta.dir, '..'))
    console.log(`PASS: ${count} server integration targets occur exactly once in CI`)
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    process.exitCode = 1
  }
}
