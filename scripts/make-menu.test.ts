import { afterEach, describe, expect, it } from 'bun:test'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import {
  buildMenuDisplayLabel,
  FEATURED_COMMAND_KEYS,
  getCommandByKey,
  getMenuColumnWidths,
  getSelectedCommandFromFzfResult,
  orderCommandsForMenu,
  readRecentCommands,
  recordRecentCommand
} from './make-menu'

describe('command discovery', () => {
  it('resolves every documented Make target to a command', async () => {
    const makefile = await readFile(new URL('../Makefile', import.meta.url), 'utf8')
    const targets = makefile.split('COMMAND_TARGETS :=')[1]?.split('.PHONY:')[0]
    expect(targets).toBeDefined()
    const keys = [...(targets ?? '').matchAll(/^\s+([a-z][a-z-]*)\s*(?:\\)?$/gm)].map((match) => match[1])
    expect(keys.length).toBeGreaterThan(0)
    for (const key of keys) {
      expect(getCommandByKey(key), `Make target ${key} has no menu command`).toBeDefined()
    }
  })
})

describe('orderCommandsForMenu', () => {
  it('prioritizes recent featured commands before the default order', () => {
    const commands = FEATURED_COMMAND_KEYS.map((key) => {
      const command = getCommandByKey(key)
      if (!command) {
        throw new Error(`Missing command for key: ${key}`)
      }
      return command
    })

    const orderedCommands = orderCommandsForMenu(commands, {
      'server-run': 300,
      test: 200,
      'docs-dev': 100
    })

    expect(orderedCommands.slice(0, 4).map(({ key }) => key)).toEqual(['server-run', 'test', 'docs-dev', 'dev'])
  })
})

describe('recent command history', () => {
  let tempDirectory: string | undefined

  afterEach(async () => {
    if (tempDirectory) {
      await rm(tempDirectory, { force: true, recursive: true })
      tempDirectory = undefined
    }
  })

  it('deduplicates command keys and keeps only the latest timestamp', async () => {
    tempDirectory = await mkdtemp(join(tmpdir(), 'make-menu-test-'))
    const historyPath = join(tempDirectory, '.make-history')

    await recordRecentCommand(historyPath, 'dev', 100)
    await recordRecentCommand(historyPath, 'test', 200)
    await recordRecentCommand(historyPath, 'dev', 300)

    expect(await readFile(historyPath, 'utf8')).toBe('test\t200\ndev\t300\n')
    expect(await readRecentCommands(historyPath)).toEqual({
      dev: 300,
      test: 200
    })
  })
})

describe('menu display formatting', () => {
  it('aligns the command name and description columns with fixed widths', () => {
    const dockerLogs = getCommandByKey('docker-logs')
    const docsBrowser = getCommandByKey('docs-check-browser')

    if (!(dockerLogs && docsBrowser)) {
      throw new Error('Missing command fixtures for formatting test')
    }

    const widths = getMenuColumnWidths([dockerLogs, docsBrowser])
    const dockerLabel = buildMenuDisplayLabel(dockerLogs, widths, {})
    const docsLabel = buildMenuDisplayLabel(docsBrowser, widths, {})

    expect(dockerLabel.indexOf(dockerLogs.name)).toBe(docsLabel.indexOf(docsBrowser.name))
    expect(dockerLabel.indexOf(dockerLogs.description)).toBe(docsLabel.indexOf(docsBrowser.description))
  })
})

describe('menu selection result handling', () => {
  it('treats a zero status from spawnSync as a successful selection', () => {
    const command = getSelectedCommandFromFzfResult({
      status: 0,
      stdout: '  Workspace  test  Run the root web test suite\tbun run test\ttest'
    })

    expect(command?.key).toBe('test')
  })
})
