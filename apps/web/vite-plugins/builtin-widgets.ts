import { readFileSync } from 'node:fs'
import path from 'node:path'
import { globSync } from 'tinyglobby'
import type { Plugin, ResolvedConfig } from 'vite'
import { build as viteBuild } from 'vite'

interface BuiltinManifestEntry {
  entry_path: string
  id: string
  manifest: Record<string, unknown>
  version: string
}

interface WidgetEntry {
  manifest: Record<string, unknown>
  srcPath: string
}

const JSDOC_RE = /\/\*\*[\s\S]*?@serverbee-widget\s+(\{[\s\S]*?\})\s*\*\//
const LINE_DECOR = /^\s*\*\s?/gm
const SRC_DIR = 'src/builtin-widgets'
const EXTERNAL_IDS = ['react', 'react-dom', 'react/jsx-runtime', '@serverbee/widget-sdk']

function extractManifest(source: string): Record<string, unknown> {
  const m = source.match(JSDOC_RE)
  if (!m) {
    throw new Error('no @serverbee-widget JSDoc block')
  }
  return JSON.parse(m[1].replace(LINE_DECOR, ''))
}

function collectEntries(rootDir: string): Map<string, WidgetEntry> {
  const entries = new Map<string, WidgetEntry>()
  const files = globSync(`${SRC_DIR}/*.widget.tsx`, { cwd: rootDir, absolute: false })
  for (const rel of files) {
    const id = path.basename(rel, '.widget.tsx')
    const abs = path.resolve(rootDir, rel)
    const source = readFileSync(abs, 'utf8')
    const manifest = extractManifest(source)
    entries.set(id, { srcPath: abs, manifest })
  }
  return entries
}

/**
 * Vite plugin: compile every `apps/web/src/builtin-widgets/*.widget.tsx`
 * to a standalone ESM bundle at `dist/builtin-widgets/<id>/index.js`,
 * with `react`/`react-dom`/`react/jsx-runtime`/`@serverbee/widget-sdk`
 * marked external (provided by the host SPA at runtime). Also emits
 * `dist/builtin-widgets/manifest.json` listing every widget.
 *
 * Widgets are compiled in a separate (nested) Vite build invoked at
 * `closeBundle` of the main build, so the externals do NOT leak into
 * the main SPA chunk graph or its HTML.
 */
export function builtinWidgetsPlugin(): Plugin {
  let resolvedConfig: ResolvedConfig | undefined
  let didNestedBuild = false

  return {
    name: 'serverbee-builtin-widgets',
    apply: 'build',
    configResolved(config) {
      resolvedConfig = config
    },
    async closeBundle() {
      if (didNestedBuild || !resolvedConfig) {
        return
      }
      // Avoid recursion when this hook fires for the nested build itself.
      didNestedBuild = true

      const rootDir = resolvedConfig.root
      const outDir = path.resolve(rootDir, resolvedConfig.build.outDir, 'builtin-widgets')
      const entries = collectEntries(rootDir)
      if (entries.size === 0) {
        return
      }

      const input: Record<string, string> = {}
      for (const [id, e] of entries) {
        input[`${id}/index`] = e.srcPath
      }

      await viteBuild({
        configFile: false,
        root: rootDir,
        mode: resolvedConfig.mode,
        // Do NOT copy apps/web/public/* into the widget output directory.
        publicDir: false,
        logLevel: 'warn',
        // Reuse the same aliases (e.g. @serverbee/widget-sdk source mapping)
        // so widgets compile against the same workspace SDK as the SPA.
        resolve: {
          alias: resolvedConfig.resolve.alias
        },
        plugins: [
          {
            name: 'serverbee-builtin-widgets-manifest',
            generateBundle() {
              const list: BuiltinManifestEntry[] = []
              for (const [id, e] of entries) {
                const m = e.manifest as { id: string; version: string }
                list.push({
                  id: m.id,
                  version: m.version,
                  entry_path: `${id}/index.js`,
                  manifest: e.manifest
                })
              }
              this.emitFile({
                type: 'asset',
                fileName: 'manifest.json',
                source: JSON.stringify(list, null, 2)
              })
            }
          }
        ],
        build: {
          outDir,
          emptyOutDir: true,
          minify: resolvedConfig.build.minify,
          sourcemap: resolvedConfig.build.sourcemap,
          target: resolvedConfig.build.target,
          // Library-style multi-entry rollup build so each widget produces
          // its own ESM chunk with preserved default export.
          lib: false as unknown as undefined,
          rollupOptions: {
            input,
            external: EXTERNAL_IDS,
            preserveEntrySignatures: 'strict',
            output: {
              format: 'es',
              entryFileNames: '[name].js',
              chunkFileNames: 'shared/[name]-[hash].js'
            }
          }
        }
      })
    }
  }
}
