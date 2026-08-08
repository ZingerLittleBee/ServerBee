/**
 * Regenerates the committed boneyard artifacts (src/bones/registry.ts and
 * the *.bones.json files) through the boneyard CLI's supported flow:
 *
 *   1. Starts an ephemeral Vite dev server on a fixed port with the boneyard
 *      vite plugin's auto-capture force-disabled (BONEYARD_AUTO_CAPTURE=0)
 *      so a globally exported opt-in cannot race the CLI's output.
 *   2. Runs `boneyard-js build` against the fixture-only /boneyard-capture
 *      surface. Passing an explicit non-root URL puts the CLI in single-page
 *      mode, so capture is deterministic — no link crawling, no filesystem
 *      route scanning, no backend, no credentials, no production data.
 *
 * The CLI auto-installs its headless Chromium via Playwright on first run
 * (requires a `node` binary on PATH for that one-time install).
 *
 * Usage: bun run generate:bones
 */
import { spawn } from 'node:child_process'
import { resolve } from 'node:path'
import { createServer } from 'vite'

const CAPTURE_PORT = 5199
const webRoot = resolve(import.meta.dirname, '..')

process.env.BONEYARD_AUTO_CAPTURE = '0'

const server = await createServer({
  configFile: resolve(webRoot, 'vite.config.ts'),
  root: webRoot,
  server: { port: CAPTURE_PORT, strictPort: true }
})
await server.listen()

const cliPath = resolve(webRoot, 'node_modules/boneyard-js/bin/cli.js')
const captureUrl = `http://localhost:${CAPTURE_PORT}/boneyard-capture`

try {
  await new Promise<void>((resolvePromise, rejectPromise) => {
    const child = spawn(process.execPath, [cliPath, 'build', captureUrl], {
      cwd: webRoot,
      stdio: 'inherit'
    })
    child.on('error', rejectPromise)
    child.on('exit', (code) => {
      if (code === 0) {
        resolvePromise()
      } else {
        rejectPromise(new Error(`boneyard-js build exited with code ${code ?? 'unknown'}`))
      }
    })
  })
} finally {
  await server.close()
}
