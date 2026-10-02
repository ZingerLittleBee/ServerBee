import { afterEach, describe, expect, it } from 'bun:test'
import { mkdir, mkdtemp, rm, symlink, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { integrationTargets, validateIntegrationTargets } from './check-integration-targets'

function workflow(agent: string, api = '--test router_smoke'): string {
  return `jobs:
  rust-tests-server:
    strategy:
      matrix:
        include:
          - group: agent
            tests: ${JSON.stringify(agent)}
          - group: api
            tests: ${JSON.stringify(api)}
`
}

const targets = ['agent_messages', 'agent_reply_binding', 'router_smoke']

describe('server integration matrix coverage', () => {
  it('accepts each target exactly once across groups', () => {
    expect(() =>
      validateIntegrationTargets(targets, workflow('--test agent_messages --test=agent_reply_binding'))
    ).not.toThrow()
  })

  it('rejects a missing security regression target', () => {
    expect(() => validateIntegrationTargets(targets, workflow('--test agent_messages'))).toThrow(
      'Missing integration targets: agent_reply_binding'
    )
  })

  it('rejects a target selected by more than one group', () => {
    expect(() =>
      validateIntegrationTargets(
        targets,
        workflow('--test agent_messages --test agent_reply_binding', '--test router_smoke --test agent_messages')
      )
    ).toThrow('Duplicate integration selectors: agent_messages')
  })

  it('rejects a selector left behind after a target was removed', () => {
    expect(() =>
      validateIntegrationTargets(
        targets,
        workflow('--test agent_messages --test agent_reply_binding --test deleted_suite')
      )
    ).toThrow('Unknown integration selectors: deleted_suite')
  })

  it('rejects an absent matrix and selectors without a target', () => {
    expect(() => validateIntegrationTargets(targets, 'jobs: {}')).toThrow('rust-tests-server job must be an object')
    expect(() => validateIntegrationTargets(targets, workflow('--test'))).toThrow('Invalid server test selector')
  })
})

describe('Cargo integration target discovery', () => {
  let crate: string | undefined

  afterEach(async () => {
    if (crate) {
      await rm(crate, { force: true, recursive: true })
      crate = undefined
    }
  })

  it('discovers file and directory targets while ignoring shared helpers', async () => {
    crate = await mkdtemp(join(tmpdir(), 'integration-targets-'))
    await mkdir(join(crate, 'tests/nested'), { recursive: true })
    await mkdir(join(crate, 'tests/common'))
    await writeFile(join(crate, 'Cargo.toml'), '[package]\nname = "fixture"\n')
    await writeFile(join(crate, 'tests/direct.rs'), '')
    await writeFile(join(crate, 'tests/nested/main.rs'), '')
    await writeFile(join(crate, 'tests/common/mod.rs'), '')

    expect(await integrationTargets(crate)).toEqual(['direct', 'nested'])
  })

  it('respects autotests = false and explicit test names', async () => {
    crate = await mkdtemp(join(tmpdir(), 'integration-targets-'))
    await mkdir(join(crate, 'tests'))
    await writeFile(join(crate, 'tests/ignored.rs'), '')
    await writeFile(
      join(crate, 'Cargo.toml'),
      '[package]\nname = "fixture"\nautotests = false\n[[test]]\nname = "custom"\n'
    )

    expect(await integrationTargets(crate)).toEqual(['custom'])
  })

  it('uses an explicit name instead of auto-discovering the same source twice', async () => {
    crate = await mkdtemp(join(tmpdir(), 'integration-targets-'))
    await mkdir(join(crate, 'tests'))
    await writeFile(join(crate, 'tests/source.rs'), '')
    await writeFile(
      join(crate, 'Cargo.toml'),
      '[package]\nname = "fixture"\nedition = "2024"\n[[test]]\nname = "custom"\npath = "tests/source.rs"\n'
    )

    expect(await integrationTargets(crate)).toEqual(['custom'])
  })

  it('allows explicit tests outside a tests directory', async () => {
    crate = await mkdtemp(join(tmpdir(), 'integration-targets-'))
    await writeFile(
      join(crate, 'Cargo.toml'),
      '[package]\nname = "fixture"\n[[test]]\nname = "custom"\npath = "src/check.rs"\n'
    )

    expect(await integrationTargets(crate)).toEqual(['custom'])
  })

  it('discovers symlinked sources and directories so unlisted targets cannot pass coverage', async () => {
    crate = await mkdtemp(join(tmpdir(), 'integration-targets-'))
    await mkdir(join(crate, 'tests'))
    await mkdir(join(crate, 'sources/nested'), { recursive: true })
    await writeFile(join(crate, 'Cargo.toml'), '[package]\nname = "fixture"\nedition = "2024"\n')
    await writeFile(join(crate, 'sources/security.rs'), '')
    await writeFile(join(crate, 'sources/nested/main.rs'), '')
    await symlink('../sources/security.rs', join(crate, 'tests/security.rs'))
    await symlink('../sources/nested', join(crate, 'tests/nested'))

    const discovered = await integrationTargets(crate)
    expect(discovered).toEqual(['nested', 'security'])
    expect(() => validateIntegrationTargets(discovered, workflow('--test nested', '--test nested'))).toThrow(
      'Missing integration targets: security'
    )
  })

  it('disables default discovery for Cargo 2015 with an explicit target', async () => {
    crate = await mkdtemp(join(tmpdir(), 'integration-targets-'))
    await mkdir(join(crate, 'tests'))
    await writeFile(join(crate, 'tests/other.rs'), '')
    await writeFile(join(crate, 'tests/source.rs'), '')
    const manifest = '[package]\nname = "fixture"\n[[test]]\nname = "custom"\npath = "tests/source.rs"\n'
    await writeFile(join(crate, 'Cargo.toml'), manifest)

    expect(await integrationTargets(crate)).toEqual(['custom'])

    await writeFile(join(crate, 'Cargo.toml'), manifest.replace('[package]\n', '[package]\nautotests = true\n'))
    expect(await integrationTargets(crate)).toEqual(['custom', 'other'])
  })

  it('resolves an inherited workspace edition and rejects an unknown edition', async () => {
    crate = await mkdtemp(join(tmpdir(), 'integration-targets-'))
    const member = join(crate, 'member')
    await mkdir(join(member, 'tests'), { recursive: true })
    await writeFile(join(member, 'tests/automatic.rs'), '')
    await writeFile(join(member, 'tests/source.rs'), '')
    await writeFile(join(crate, 'Cargo.toml'), '[workspace.package]\nedition = "2024"\n')
    await writeFile(
      join(member, 'Cargo.toml'),
      '[package]\nname = "fixture"\nedition.workspace = true\n[[test]]\nname = "custom"\npath = "tests/source.rs"\n'
    )

    expect(await integrationTargets(member)).toEqual(['automatic', 'custom'])

    await writeFile(join(crate, 'Cargo.toml'), '[workspace.package]\nedition = "2099"\n')
    await expect(integrationTargets(member)).rejects.toThrow('Unsupported Cargo edition: 2099')
  })
})
