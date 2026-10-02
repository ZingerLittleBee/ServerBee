import { afterEach, describe, expect, it } from 'bun:test'
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { checkAgentNavigation } from './check-agent-navigation'

describe('agent navigation contracts', () => {
  let repository: string | undefined

  afterEach(async () => {
    if (repository) {
      await rm(repository, { recursive: true, force: true })
      repository = undefined
    }
  })

  async function fixture(files: Record<string, string>): Promise<string> {
    repository = await mkdtemp(join(tmpdir(), 'serverbee-navigation-'))
    for (const [file, contents] of Object.entries(files)) {
      const path = join(repository, file)
      await mkdir(dirname(path), { recursive: true })
      await writeFile(path, contents)
    }
    return repository
  }

  it('resolves links from the document directory and ignores external URLs and fragments', async () => {
    const root = await fixture({
      'docs/agents/navigation.md':
        '[source](../../crates/server/src/state.rs#appstate)\n[web](https://example.com/a)\n[section](#task-map)',
      'crates/server/src/state.rs': 'pub struct AppState;'
    })
    expect(
      await checkAgentNavigation(root, { documents: ['docs/agents/navigation.md'], historicalDocuments: [] })
    ).toEqual([])
  })

  it('fails when a source is moved without updating its navigation link', async () => {
    const root = await fixture({ 'AGENTS.md': '[old source](crates/server/src/old.rs)' })
    expect(await checkAgentNavigation(root, { documents: ['AGENTS.md'], historicalDocuments: [] })).toContain(
      'AGENTS.md: missing navigation target: crates/server/src/old.rs'
    )
  })

  it('rejects an unsupported locale even when its directory exists', async () => {
    const root = await fixture({
      'AGENTS.md': 'Read `apps/docs/content/docs/cn/configuration.mdx`.',
      'apps/docs/content/docs/cn/configuration.mdx': '# Old locale'
    })
    expect(await checkAgentNavigation(root, { documents: ['AGENTS.md'], historicalDocuments: [] })).toContain(
      'AGENTS.md: unsupported docs locale cn; use zh'
    )
  })

  it('checks app-local source pointers and scripts from the app README', async () => {
    const root = await fixture({
      'apps/docs/README.md': 'Start with `src/routes/index.tsx` and `scripts/check-browser.ts`.',
      'apps/docs/src/routes/index.tsx': 'export const Route = {}'
    })
    expect(await checkAgentNavigation(root, { documents: ['apps/docs/README.md'], historicalDocuments: [] })).toEqual([
      'apps/docs/README.md: missing navigation target: scripts/check-browser.ts'
    ])
  })

  it('accepts both supported app-local content directories', async () => {
    const root = await fixture({
      'apps/docs/README.md': 'Content lives in `content/docs/{en,zh}`.',
      'apps/docs/content/docs/en/index.mdx': '# English',
      'apps/docs/content/docs/zh/index.mdx': '# Chinese'
    })
    expect(await checkAgentNavigation(root, { documents: ['apps/docs/README.md'], historicalDocuments: [] })).toEqual(
      []
    )
  })

  it('distinguishes a directory glob from a concrete source pointer', async () => {
    const root = await fixture({ 'AGENTS.md': 'The `packages/*` directories are scaffold packages.' })
    expect(await checkAgentNavigation(root, { documents: ['AGENTS.md'], historicalDocuments: [] })).toEqual([])
  })

  it('checks every brace-expanded locale instead of accepting a missing bilingual page', async () => {
    const root = await fixture({
      'AGENTS.md': 'Update `apps/docs/content/docs/{en,zh}/configuration.mdx`.',
      'apps/docs/content/docs/en/configuration.mdx': '# Configuration'
    })
    expect(await checkAgentNavigation(root, { documents: ['AGENTS.md'], historicalDocuments: [] })).toContain(
      'AGENTS.md: missing navigation target: apps/docs/content/docs/zh/configuration.mdx'
    )
  })

  it('catches the old en,cn shortcut from release instructions', async () => {
    const root = await fixture({
      'AGENTS.md': 'Update `apps/docs/content/docs/{en,cn}/configuration.mdx`.',
      'apps/docs/content/docs/en/configuration.mdx': '# Configuration'
    })
    const issues = await checkAgentNavigation(root, { documents: ['AGENTS.md'], historicalDocuments: [] })
    expect(issues).toContain('AGENTS.md: unsupported docs locale cn; use zh')
    expect(issues).toContain('AGENTS.md: missing navigation target: apps/docs/content/docs/cn/configuration.mdx')
  })

  it('requires a visible historical marker and accepts one near the title', async () => {
    const root = await fixture({
      'docs/old.md': '# Old design\n\nOutdated behavior',
      'docs/archive.md': '# Archived design\n\n<!-- agent-navigation: historical -->\n'
    })
    expect(
      await checkAgentNavigation(root, { documents: [], historicalDocuments: ['docs/old.md', 'docs/archive.md'] })
    ).toEqual(['docs/old.md: add the historical marker near the document title'])
  })

  it('does not accept a marker buried below the historical document header', async () => {
    const root = await fixture({
      'docs/old.md': `# Old design\n${'Historical detail\n'.repeat(25)}<!-- agent-navigation: historical -->`
    })
    expect(await checkAgentNavigation(root, { documents: [], historicalDocuments: ['docs/old.md'] })).toEqual([
      'docs/old.md: add the historical marker near the document title'
    ])
  })

  it('reports deleted entry documents instead of silently checking nothing', async () => {
    const root = await fixture({ 'README.md': '# Repo' })
    expect(await checkAgentNavigation(root, { documents: ['AGENTS.md'], historicalDocuments: [] })).toEqual([
      'AGENTS.md: navigation document is missing or unreadable'
    ])
  })

  it('rejects machine-specific links and paths outside the checkout', async () => {
    const root = await fixture({ 'AGENTS.md': '[outside](../other-repo/README.md)\n[local](/tmp/source.rs)' })
    expect(await checkAgentNavigation(root, { documents: ['AGENTS.md'], historicalDocuments: [] })).toEqual([
      'AGENTS.md: link escapes the repository: ../other-repo/README.md',
      'AGENTS.md: use a portable relative repository link: /tmp/source.rs'
    ])
  })
})
