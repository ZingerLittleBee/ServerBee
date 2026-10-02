import { createReadStream } from 'node:fs'
import { access, realpath, stat } from 'node:fs/promises'
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http'
import { extname, resolve, sep } from 'node:path'
import { pipeline } from 'node:stream/promises'
import { fileURLToPath, pathToFileURL } from 'node:url'

const docsApp = fileURLToPath(new URL('..', import.meta.url))
const output = resolve(docsApp, '.vercel/output')
const entry = resolve(output, 'functions/__server.func/index.mjs')
const host = process.env.HOST ?? '127.0.0.1'
const port = Number(process.env.PORT ?? 4000)

if (!Number.isInteger(port) || port < 1 || port > 65_535) {
  throw new Error('PORT must be an integer between 1 and 65535')
}

try {
  await access(entry)
} catch (cause) {
  throw new Error('The docs production build is missing. Run `bun run build` in apps/docs first.', { cause })
}

process.env.NODE_ENV ??= 'production'
const staticRoot = await realpath(resolve(output, 'static'))
const { default: handler } = await import(pathToFileURL(entry).href)
if (typeof handler !== 'function') {
  throw new Error('The Vercel build must export a Node.js request handler (vercel.entryFormat: node)')
}

const contentTypes: Record<string, string> = {
  '.css': 'text/css; charset=utf-8',
  '.html': 'text/html; charset=utf-8',
  '.ico': 'image/x-icon',
  '.jpeg': 'image/jpeg',
  '.jpg': 'image/jpeg',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.txt': 'text/plain; charset=utf-8',
  '.webp': 'image/webp',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.xml': 'application/xml; charset=utf-8'
}

async function serveStatic(request: IncomingMessage, response: ServerResponse): Promise<boolean> {
  if (request.method !== 'GET' && request.method !== 'HEAD') {
    return false
  }
  let pathname: string
  try {
    pathname = decodeURIComponent(new URL(request.url ?? '/', 'http://localhost').pathname)
  } catch {
    return false
  }
  if (pathname.split('/').some((segment) => segment.startsWith('.'))) {
    return false
  }
  const filename = resolve(staticRoot, `.${pathname}`)
  if (!filename.startsWith(`${staticRoot}${sep}`)) {
    return false
  }
  try {
    const actual = await realpath(filename)
    if (!actual.startsWith(`${staticRoot}${sep}`)) {
      return false
    }
    const metadata = await stat(actual)
    if (!metadata.isFile()) {
      return false
    }
    response.setHeader('Content-Type', contentTypes[extname(actual)] ?? 'application/octet-stream')
    response.setHeader('Content-Length', metadata.size)
    response.setHeader('Last-Modified', metadata.mtime.toUTCString())
    if (pathname.startsWith('/assets/')) {
      // Matches the hashed asset header in .vercel/output/config.json.
      response.setHeader('Cache-Control', 'public, max-age=31536000, immutable')
    }
    if (request.method === 'HEAD') {
      response.end()
    } else {
      await pipeline(createReadStream(actual), response)
    }
    return true
  } catch (error) {
    if (error instanceof Error && 'code' in error && (error.code === 'ENOENT' || error.code === 'ENOTDIR')) {
      return false
    }
    throw error
  }
}

const server = createServer((request, response) => {
  const respond = async () => {
    if (!(await serveStatic(request, response))) {
      await handler(request, response)
    }
  }
  respond().catch((error: unknown) => {
    // Navigation can cancel a static download. pipeline closes the file stream, and that client disconnect is normal.
    if (
      response.destroyed &&
      error instanceof Error &&
      'code' in error &&
      error.code === 'ERR_STREAM_PREMATURE_CLOSE'
    ) {
      return
    }
    console.error(error)
    if (response.headersSent) {
      response.destroy()
    } else {
      response.writeHead(500, { 'Content-Type': 'text/plain; charset=utf-8' })
      response.end('Production preview failed; see the server log.\n')
    }
  })
})

// Nitro's Vercel handler defines socket.remoteAddress once per invocation. Reusing that socket throws on the next
// request, so close each response's connection without changing the generated handler or adding a proxy process.
server.maxRequestsPerSocket = 1
server.listen(port, host, () => {
  console.log(`Docs production preview: http://${host}:${port}`)
})
for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.once(signal, () => server.close())
}
