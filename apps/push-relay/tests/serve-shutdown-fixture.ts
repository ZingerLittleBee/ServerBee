/** Isolated subprocess: real main.ts/Bun HTTP server, deterministic work instead of live APNs. */
import { mock } from 'bun:test'
import { appendFileSync } from 'node:fs'

const record = (event: string) => appendFileSync(process.env.SHUTDOWN_EVENTS ?? '', `${event}\n`)

mock.module('../src/apns', () => ({
  ApnsTransport: class {
    close() {
      record('apns-close')
    }
  }
}))
mock.module('../src/relay', () => ({
  Relay: class {
    readonly db = {
      close() {
        record('database-close')
      }
    }
    async handle(): Promise<Response> {
      record('handler-start')
      if (process.env.SHUTDOWN_MODE === 'stalled') {
        await new Promise<void>(() => {
          /* Deliberately uncooperative request handler. */
        })
      } else {
        await new Promise<void>((resolve) => setTimeout(resolve, 200))
      }
      record('handler-finish')
      return new Response('done')
    }
  }
}))
await import('../src/main')
record('ready')
