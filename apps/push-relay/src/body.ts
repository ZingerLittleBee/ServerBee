/** Bounded, cancellable reads. Neither upload nor provider bodies use text()/json(). */
export class BodyError extends Error {
  readonly kind: 'too_large' | 'timeout' | 'invalid'

  constructor(kind: 'too_large' | 'timeout' | 'invalid') {
    super(kind)
    this.kind = kind
  }
}

export function cancelBody(body: ReadableStream<Uint8Array> | null): void {
  // A hostile source may never settle cancel(); do not await it on a rejection path.
  body?.cancel().catch(() => undefined)
}

export async function deadline<T>(
  milliseconds: number,
  external: AbortSignal | undefined,
  operation: (signal: AbortSignal) => Promise<T>
): Promise<T> {
  const controller = new AbortController()
  let timer: ReturnType<typeof setTimeout> | undefined
  let abort: () => void = () => undefined
  const aborted = new Promise<never>((_resolve, reject) => {
    abort = () => {
      controller.abort()
      reject(new BodyError('timeout'))
    }
    external?.addEventListener('abort', abort, { once: true })
    timer = setTimeout(abort, milliseconds)
  })
  try {
    if (external?.aborted) {
      abort()
    }
    return await Promise.race([aborted, operation(controller.signal)])
  } finally {
    if (timer !== undefined) {
      clearTimeout(timer)
    }
    external?.removeEventListener('abort', abort)
  }
}

export async function readBytes(
  body: ReadableStream<Uint8Array> | null,
  maxBytes: number,
  signal: AbortSignal
): Promise<Uint8Array> {
  if (!body) {
    return new Uint8Array()
  }
  const reader = body.getReader()
  const bytes = new Uint8Array(maxBytes)
  let size = 0
  let chunks = 0
  let finished = false
  let rejectAbort: (reason: Error) => void = () => undefined
  const interrupted = new Promise<never>((_resolve, reject) => {
    rejectAbort = reject
  })
  const abort = () => {
    reader.cancel().catch(() => undefined)
    rejectAbort(new BodyError('timeout'))
  }
  signal.addEventListener('abort', abort, { once: true })
  try {
    if (signal.aborted) {
      throw new BodyError('timeout')
    }
    for (;;) {
      const { value, done } = await Promise.race([reader.read(), interrupted])
      if (done) {
        finished = true
        return bytes.subarray(0, size)
      }
      chunks += 1
      // A hard chunk cap also bounds empty/tiny-chunk bookkeeping.
      if (!(value instanceof Uint8Array) || chunks > maxBytes + 1) {
        throw new BodyError('invalid')
      }
      if (value.byteLength > maxBytes - size) {
        throw new BodyError('too_large')
      }
      bytes.set(value, size)
      size += value.byteLength
    }
  } finally {
    signal.removeEventListener('abort', abort)
    if (!finished) {
      reader.cancel().catch(() => undefined)
    }
    reader.releaseLock()
  }
}
