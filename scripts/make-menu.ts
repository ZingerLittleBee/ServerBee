import { spawn, spawnSync } from 'node:child_process'
import { readFile, writeFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

export interface CommandDefinition {
  category: string
  command: string
  description: string
  featured?: boolean
  key: string
  name: string
}

const COMMANDS: CommandDefinition[] = [
  {
    key: 'install',
    name: 'install',
    category: 'Workspace',
    description: 'Install workspace dependencies',
    command: 'bun install'
  },
  {
    key: 'dev',
    name: 'dev',
    category: 'Workspace',
    description: 'Start the full Turbo development workflow',
    command: 'bun run dev',
    featured: true
  },
  {
    key: 'dev-web',
    name: 'dev:web',
    category: 'Workspace',
    description: 'Start the web app through the root workspace entrypoint',
    command: 'bun run dev:web',
    featured: true
  },
  {
    key: 'build',
    name: 'build',
    category: 'Workspace',
    description: 'Build the workspace through Turbo',
    command: 'bun run build',
    featured: true
  },
  {
    key: 'build-web',
    name: 'build:web',
    category: 'Workspace',
    description: 'Build only the web app from the root workspace',
    command: 'bun run build:web'
  },
  {
    key: 'typecheck',
    name: 'typecheck',
    category: 'Workspace',
    description: 'Run the root type-check aggregation',
    command: 'bun run typecheck',
    featured: true
  },
  {
    key: 'check-types',
    name: 'check-types',
    category: 'Workspace',
    description: 'Alias for the root type-check command',
    command: 'bun run check-types'
  },
  {
    key: 'lint',
    name: 'lint',
    category: 'Workspace',
    description: 'Run the root lint/check command',
    command: 'bun run lint',
    featured: true
  },
  {
    key: 'check',
    name: 'check',
    category: 'Workspace',
    description: 'Run Ultracite in check mode',
    command: 'bun run check'
  },
  {
    key: 'fix',
    name: 'fix',
    category: 'Workspace',
    description: 'Run Ultracite auto-fixes',
    command: 'bun run fix',
    featured: true
  },
  {
    key: 'test',
    name: 'test',
    category: 'Workspace',
    description: 'Run the root web test suite',
    command: 'bun run test',
    featured: true
  },
  {
    key: 'test-watch',
    name: 'test:watch',
    category: 'Workspace',
    description: 'Run the root web test suite in watch mode',
    command: 'bun run test:watch'
  },
  {
    key: 'web-dev',
    name: 'web:dev',
    category: 'Web',
    description: 'Start the Vite web dev server directly',
    command: 'bun --filter @serverbee/web dev'
  },
  {
    key: 'web-build',
    name: 'web:build',
    category: 'Web',
    description: 'Build the web app directly',
    command: 'bun --filter @serverbee/web build'
  },
  {
    key: 'web-typecheck',
    name: 'web:typecheck',
    category: 'Web',
    description: 'Type-check the web app directly',
    command: 'bun --filter @serverbee/web typecheck'
  },
  {
    key: 'web-preview',
    name: 'web:preview',
    category: 'Web',
    description: 'Preview the built web app',
    command: 'bun --filter @serverbee/web preview'
  },
  {
    key: 'web-test',
    name: 'web:test',
    category: 'Web',
    description: 'Run the web Vitest suite directly',
    command: 'bun --filter @serverbee/web test'
  },
  {
    key: 'web-test-watch',
    name: 'web:test:watch',
    category: 'Web',
    description: 'Run the web Vitest suite in watch mode',
    command: 'bun --filter @serverbee/web test:watch'
  },
  {
    key: 'web-generate-api-types',
    name: 'web:generate-api-types',
    category: 'Web',
    description: 'Regenerate OpenAPI-based frontend types',
    command: 'bun --filter @serverbee/web generate:api-types'
  },
  {
    key: 'docs-dev',
    name: 'docs:dev',
    category: 'Docs',
    description: 'Start the docs development server',
    command: 'bun --filter @serverbee/docs dev',
    featured: true
  },
  {
    key: 'docs-build',
    name: 'docs:build',
    category: 'Docs',
    description: 'Build the docs app',
    command: 'bun --filter @serverbee/docs build',
    featured: true
  },
  {
    key: 'docs-start',
    name: 'docs:start',
    category: 'Docs',
    description: 'Start the built docs app',
    command: 'bun --filter @serverbee/docs start'
  },
  {
    key: 'docs-preview',
    name: 'docs:preview',
    category: 'Docs',
    description: 'Preview the built docs app',
    command: 'bun --filter @serverbee/docs preview'
  },
  {
    key: 'docs-typecheck',
    name: 'docs:typecheck',
    category: 'Docs',
    description: 'Type-check the docs app',
    command: 'bun --filter @serverbee/docs types:check'
  },
  {
    key: 'docs-lint',
    name: 'docs:lint',
    category: 'Docs',
    description: 'Run docs lint checks',
    command: 'bun --filter @serverbee/docs lint'
  },
  {
    key: 'docs-format',
    name: 'docs:format',
    category: 'Docs',
    description: 'Format docs app files',
    command: 'bun --filter @serverbee/docs format'
  },
  {
    key: 'db-local',
    name: 'db:local',
    category: 'Database',
    description: 'Start the local Turso database',
    command: 'bun run db:local',
    featured: true
  },
  {
    key: 'db-push',
    name: 'db:push',
    category: 'Database',
    description: 'Push the Drizzle schema to the database',
    command: 'bun run db:push'
  },
  {
    key: 'db-generate',
    name: 'db:generate',
    category: 'Database',
    description: 'Generate Drizzle migration files',
    command: 'bun run db:generate',
    featured: true
  },
  {
    key: 'db-migrate',
    name: 'db:migrate',
    category: 'Database',
    description: 'Run pending Drizzle migrations',
    command: 'bun run db:migrate',
    featured: true
  },
  {
    key: 'db-studio',
    name: 'db:studio',
    category: 'Database',
    description: 'Open Drizzle Studio',
    command: 'bun run db:studio'
  },
  {
    key: 'db-local-direct',
    name: 'db:local:direct',
    category: 'Database',
    description: 'Start the local Turso database directly from the package',
    command: 'bun --filter @serverbee/db db:local'
  },
  {
    key: 'db-push-direct',
    name: 'db:push:direct',
    category: 'Database',
    description: 'Push the Drizzle schema directly from the package',
    command: 'bun --filter @serverbee/db db:push'
  },
  {
    key: 'db-generate-direct',
    name: 'db:generate:direct',
    category: 'Database',
    description: 'Generate Drizzle migrations directly from the package',
    command: 'bun --filter @serverbee/db db:generate'
  },
  {
    key: 'db-migrate-direct',
    name: 'db:migrate:direct',
    category: 'Database',
    description: 'Run Drizzle migrations directly from the package',
    command: 'bun --filter @serverbee/db db:migrate'
  },
  {
    key: 'db-studio-direct',
    name: 'db:studio:direct',
    category: 'Database',
    description: 'Open Drizzle Studio directly from the package',
    command: 'bun --filter @serverbee/db db:studio'
  },
  {
    key: 'ui-typecheck',
    name: 'ui:typecheck',
    category: 'Packages',
    description: 'Type-check the shared UI package',
    command: 'bun --filter @serverbee/ui check-types'
  },
  {
    key: 'cargo-build',
    name: 'cargo:build',
    category: 'Rust',
    description: 'Build the Rust workspace in debug mode',
    command: 'cargo build'
  },
  {
    key: 'cargo-build-release',
    name: 'cargo:build:release',
    category: 'Rust',
    description: 'Build the Rust workspace in release mode',
    command: 'cargo build --release'
  },
  {
    key: 'cargo-check',
    name: 'cargo:check',
    category: 'Rust',
    description: 'Run cargo check for the full workspace',
    command: 'cargo check --workspace'
  },
  {
    key: 'cargo-test',
    name: 'cargo:test',
    category: 'Rust',
    description: 'Run cargo test for the full workspace',
    command: 'cargo test --workspace'
  },
  {
    key: 'cargo-clippy',
    name: 'cargo:clippy',
    category: 'Rust',
    description: 'Run clippy with warnings treated as errors',
    command: 'cargo clippy --workspace --all-targets -- -D warnings'
  },
  {
    key: 'cargo-fmt',
    name: 'cargo:fmt',
    category: 'Rust',
    description: 'Format the Rust workspace',
    command: 'cargo fmt --all'
  },
  {
    key: 'server-build',
    name: 'server:build',
    category: 'Rust',
    description: 'Build the Rust server binary',
    command: 'cargo build -p serverbee-server'
  },
  {
    key: 'server-run',
    name: 'server:run',
    category: 'Rust',
    description: 'Run the Rust server binary',
    command: 'cargo run -p serverbee-server',
    featured: true
  },
  {
    key: 'agent-build',
    name: 'agent:build',
    category: 'Rust',
    description: 'Build the Rust agent binary',
    command: 'cargo build -p serverbee-agent'
  },
  {
    key: 'agent-run',
    name: 'agent:run',
    category: 'Rust',
    description: 'Run the Rust agent binary',
    command: 'cargo run -p serverbee-agent',
    featured: true
  },
  {
    key: 'docker-build',
    name: 'docker:build',
    category: 'Docker',
    description: 'Build the Docker image via docker compose',
    command: 'docker compose build'
  },
  {
    key: 'docker-up',
    name: 'docker:up',
    category: 'Docker',
    description: 'Start the Docker compose stack in the background',
    command: 'docker compose up -d',
    featured: true
  },
  {
    key: 'docker-down',
    name: 'docker:down',
    category: 'Docker',
    description: 'Stop the Docker compose stack',
    command: 'docker compose down'
  },
  {
    key: 'docker-logs',
    name: 'docker:logs',
    category: 'Docker',
    description: 'Follow logs from the Docker compose stack',
    command: 'docker compose logs -f',
    featured: true
  }
]

export const FEATURED_COMMAND_KEYS = COMMANDS.filter((command) => command.featured).map((command) => command.key)

const COMMANDS_BY_KEY = new Map(COMMANDS.map((command) => [command.key, command] as const))

const DEFAULT_ORDER_BY_KEY = new Map(FEATURED_COMMAND_KEYS.map((key, index) => [key, index] as const))

const ROOT_DIRECTORY = dirname(dirname(fileURLToPath(import.meta.url)))
const HISTORY_PATH = join(ROOT_DIRECTORY, '.make-history')

export interface MenuColumnWidths {
  categoryWidth: number
  nameWidth: number
}

interface FzfSelectionResult {
  status: number | null
  stdout: string
}

export const getCommandByKey = (key: string): CommandDefinition | undefined => COMMANDS_BY_KEY.get(key)

const getFeaturedCommands = (): CommandDefinition[] => COMMANDS.filter((command) => command.featured)

export const getMenuColumnWidths = (commands: CommandDefinition[]): MenuColumnWidths => ({
  categoryWidth: Math.max(...commands.map((command) => command.category.length)),
  nameWidth: Math.max(...commands.map((command) => command.name.length))
})

export const orderCommandsForMenu = (
  commands: CommandDefinition[],
  recentCommands: Record<string, number>
): CommandDefinition[] =>
  [...commands].sort((leftCommand, rightCommand) => {
    const leftTimestamp = recentCommands[leftCommand.key] ?? 0
    const rightTimestamp = recentCommands[rightCommand.key] ?? 0

    if (leftTimestamp !== rightTimestamp) {
      return rightTimestamp - leftTimestamp
    }

    const leftDefaultOrder = DEFAULT_ORDER_BY_KEY.get(leftCommand.key)
    const rightDefaultOrder = DEFAULT_ORDER_BY_KEY.get(rightCommand.key)

    if (leftDefaultOrder !== undefined && rightDefaultOrder !== undefined) {
      return leftDefaultOrder - rightDefaultOrder
    }

    if (leftCommand.category !== rightCommand.category) {
      return leftCommand.category.localeCompare(rightCommand.category)
    }

    return leftCommand.name.localeCompare(rightCommand.name)
  })

export const readRecentCommands = async (historyPath = HISTORY_PATH): Promise<Record<string, number>> => {
  try {
    const historyContent = await readFile(historyPath, 'utf8')

    return historyContent
      .split('\n')
      .filter(Boolean)
      .reduce<Record<string, number>>((recentCommands, line) => {
        const [key, timestampValue] = line.split('\t')
        const timestamp = Number(timestampValue)

        if (key && Number.isFinite(timestamp)) {
          recentCommands[key] = timestamp
        }

        return recentCommands
      }, {})
  } catch (error) {
    if (error instanceof Error && 'code' in error && error.code === 'ENOENT') {
      return {}
    }

    throw error
  }
}

export const recordRecentCommand = async (historyPath: string, key: string, timestamp: number): Promise<void> => {
  const recentCommands = await readRecentCommands(historyPath)
  recentCommands[key] = timestamp

  const historyLines = Object.entries(recentCommands)
    .sort((leftEntry, rightEntry) => leftEntry[1] - rightEntry[1])
    .map(([entryKey, entryTimestamp]) => `${entryKey}\t${entryTimestamp}`)

  await writeFile(historyPath, `${historyLines.join('\n')}\n`, 'utf8')
}

export const buildMenuDisplayLabel = (
  command: CommandDefinition,
  widths: MenuColumnWidths,
  recentCommands: Record<string, number>
): string => {
  const recentMarker = recentCommands[command.key] ? '*' : ' '
  return `${recentMarker} ${command.category.padEnd(widths.categoryWidth)}  ${command.name.padEnd(widths.nameWidth)}  ${command.description}`
}

const formatMenuLine = (
  command: CommandDefinition,
  widths: MenuColumnWidths,
  recentCommands: Record<string, number>
): string => [buildMenuDisplayLabel(command, widths, recentCommands), command.command, command.key].join('\t')

const ensureFzfIsInstalled = (): void => {
  const result = spawnSync('sh', ['-lc', 'command -v fzf'], {
    cwd: ROOT_DIRECTORY,
    stdio: 'ignore'
  })

  if (result.status !== 0) {
    throw new Error('fzf is required but was not found in PATH.')
  }
}

export const getSelectedCommandFromFzfResult = (result: FzfSelectionResult): CommandDefinition | undefined => {
  if (result.status !== 0) {
    return undefined
  }

  const selectedLine = result.stdout.trim()

  if (!selectedLine) {
    return undefined
  }

  const selectedKey = selectedLine.split('\t').at(-1)

  return selectedKey ? getCommandByKey(selectedKey) : undefined
}

const openMenu = async (): Promise<CommandDefinition | undefined> => {
  ensureFzfIsInstalled()

  const recentCommands = await readRecentCommands()
  const menuCommands = orderCommandsForMenu(getFeaturedCommands(), recentCommands)
  const widths = getMenuColumnWidths(menuCommands)
  const menuInput = menuCommands.map((command) => formatMenuLine(command, widths, recentCommands)).join('\n')

  const previewCommand = String.raw`printf '%s\n' {5}`
  const result = spawnSync(
    'fzf',
    [
      '--ansi',
      '--delimiter',
      '\t',
      '--with-nth',
      '1',
      '--preview',
      previewCommand,
      '--preview-window',
      'down,3,wrap',
      '--prompt',
      'make > ',
      '--header',
      'Recent commands are pinned first. * marks commands from history.'
    ],
    {
      cwd: ROOT_DIRECTORY,
      input: menuInput,
      encoding: 'utf8',
      stdio: ['pipe', 'pipe', 'inherit']
    }
  )

  return getSelectedCommandFromFzfResult({
    status: result.status,
    stdout: result.stdout
  })
}

const runShellCommand = async (command: CommandDefinition): Promise<number> => {
  await recordRecentCommand(HISTORY_PATH, command.key, Date.now())

  return new Promise<number>((resolve, reject) => {
    const childProcess = spawn(command.command, {
      cwd: ROOT_DIRECTORY,
      shell: true,
      stdio: 'inherit'
    })

    childProcess.on('error', reject)
    childProcess.on('exit', (code, signal) => {
      if (signal) {
        resolve(1)
        return
      }

      resolve(code ?? 0)
    })
  })
}

const printRecentCommands = async (): Promise<void> => {
  const recentCommands = await readRecentCommands()
  const orderedEntries = Object.entries(recentCommands).sort((leftEntry, rightEntry) => rightEntry[1] - leftEntry[1])

  if (orderedEntries.length === 0) {
    console.log('No recent commands recorded.')
    return
  }

  for (const [key, timestamp] of orderedEntries) {
    const command = getCommandByKey(key)

    if (!command) {
      continue
    }

    console.log([new Date(timestamp).toLocaleString(), command.category, command.name, command.command].join('\t'))
  }
}

const printAvailableTargets = (): void => {
  console.error('Unknown make command target.')
  console.error('')
  console.error('Available targets:')

  for (const command of COMMANDS) {
    console.error(`- ${command.key}`)
  }
}

const main = async (): Promise<void> => {
  const [action = 'menu', key] = process.argv.slice(2)

  if (action === 'recent') {
    await printRecentCommands()
    return
  }

  if (action === 'run') {
    if (!key) {
      printAvailableTargets()
      process.exitCode = 1
      return
    }

    const command = getCommandByKey(key)

    if (!command) {
      printAvailableTargets()
      process.exitCode = 1
      return
    }

    process.exitCode = await runShellCommand(command)
    return
  }

  const selectedCommand = await openMenu()

  if (!selectedCommand) {
    return
  }

  process.exitCode = await runShellCommand(selectedCommand)
}

if (import.meta.main) {
  await main()
}
