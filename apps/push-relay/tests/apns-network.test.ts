import { test } from 'bun:test'
import assert from 'node:assert/strict'
import { generateKeyPairSync } from 'node:crypto'
import { once } from 'node:events'
import { type ClientHttp2Session, connect, constants, createServer, type ServerHttp2Stream } from 'node:http2'
import { createServer as createTcpServer, type Socket } from 'node:net'
import { ApnsTransport, AppleNetwork, classify } from '../src/apns'
import type { Environment } from '../src/attestation'

const now = 1_800_000_000
const key = generateKeyPairSync('ec', { namedCurve: 'prime256v1' })
const config = {
  keyId: 'TESTKEY01',
  teamId: 'TESTTEAM01',
  topic: 'com.serverbee.mobile',
  privateKey: key.privateKey.export({ type: 'pkcs8', format: 'pem' }).toString()
}
const eventId = '45ab56c7-d890-4123-a456-123456789abc'

async function fixture(handle: (stream: ServerHttp2Stream, path: string) => void, timeoutMs = 1000) {
  const server = createServer()
  const serverSessions = new Set<ServerHttp2Stream['session']>()
  const clients: ClientHttp2Session[] = []
  const environments: Environment[] = []
  server.on('session', (session) => {
    serverSessions.add(session)
    session.on('error', () => {
      /* Expected in disconnect tests. */
    })
    session.on('close', () => serverSessions.delete(session))
  })
  server.on('stream', (stream: ServerHttp2Stream, headers) => {
    stream.on('error', () => {
      /* Expected in reset tests. */
    })
    stream.resume()
    handle(stream, String(headers[':path']))
  })
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  const address = server.address()
  assert(address && typeof address === 'object')
  const network = new AppleNetwork((environment) => {
    environments.push(environment)
    const client = connect(`http://127.0.0.1:${address.port}`)
    clients.push(client)
    return client
  }, timeoutMs)
  const transport = new ApnsTransport(config, network.send, () => now)
  return {
    clients,
    environments,
    network,
    send: (token = 'device', environment: Environment = 'sandbox') =>
      transport.send(token, environment, eventId, now + 1800, { ciphertext: 'isolated' }),
    async close() {
      network.close()
      for (const session of serverSessions) {
        session?.destroy()
      }
      await new Promise<void>((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())))
    }
  }
}

function accept(stream: ServerHttp2Stream): void {
  stream.respond({ ':status': 200 })
  stream.end()
}

const retryable = { outcome: 'retryable', reason: 'NetworkUnavailable', device_invalid: false }

test('APNs reuses one session for sequential and concurrent streams, independently per environment', async () => {
  const local = await fixture(accept)
  try {
    // All sends start before the first session has connected.
    const connecting = await Promise.all(Array.from({ length: 12 }, (_, i) => local.send(`connecting-${i}`)))
    assert(connecting.every((reply) => reply.outcome === 'accepted'))
    assert.equal((await local.send()).outcome, 'accepted')
    assert.equal((await local.send()).outcome, 'accepted')
    const replies = await Promise.all(Array.from({ length: 12 }, (_, i) => local.send(`device-${i}`)))
    assert(replies.every((reply) => reply.outcome === 'accepted'))
    assert.deepEqual(local.environments, ['sandbox'])
    assert.equal((await local.send('production-device', 'production')).outcome, 'accepted')
    assert.equal((await local.send('sandbox-device')).outcome, 'accepted')
    assert.deepEqual(local.environments, ['sandbox', 'production'])
  } finally {
    await local.close()
  }
})

for (const failure of ['reset', 'destroy', 'goaway'] as const) {
  test(`APNs ${failure} before response headers is retryable and the next send succeeds`, async () => {
    let first = true
    const local = await fixture((stream) => {
      if (!first) {
        accept(stream)
        return
      }
      first = false
      if (failure === 'reset') {
        stream.close(constants.NGHTTP2_NO_ERROR)
      } else if (failure === 'destroy') {
        stream.session?.destroy()
      } else {
        stream.session?.goaway(constants.NGHTTP2_NO_ERROR)
        stream.session?.destroy()
      }
    })
    try {
      assert.deepEqual(await local.send(), retryable)
      assert.equal((await local.send()).outcome, 'accepted')
      assert.equal(local.clients.length, failure === 'reset' ? 1 : 2)
    } finally {
      await local.close()
    }
  })
}

test('APNs session error settles all pending streams and reconnects', async () => {
  const streams: ServerHttp2Stream[] = []
  let failed = false
  const local = await fixture((stream) => {
    if (failed) {
      accept(stream)
      return
    }
    streams.push(stream)
    if (streams.length === 2) {
      failed = true
      stream.session?.destroy(new Error('Isolated provider failure'))
    }
  })
  try {
    assert.deepEqual(await Promise.all([local.send('one'), local.send('two')]), [retryable, retryable])
    assert.equal((await local.send()).outcome, 'accepted')
    assert.equal(local.clients.length, 2)
  } finally {
    await local.close()
  }
})

test('APNs GOAWAY reconnects and distinguishes completed streams from runtime closures', async () => {
  let retiring: ServerHttp2Stream | undefined
  const local = await fixture((stream, path) => {
    if (path.endsWith('/retiring')) {
      retiring = stream
      stream.session?.goaway(constants.NGHTTP2_NO_ERROR, stream.id)
    } else {
      accept(stream)
    }
  })
  try {
    const first = local.send('retiring')
    await once(local.clients[0], 'goaway')
    assert.equal((await local.send('replacement')).outcome, 'accepted')
    assert(retiring)
    if (retiring.destroyed) {
      // Bun 1.3.4 closes the session itself on GOAWAY before this response.
      // With no completed response, the only safe result is retryable.
      assert.equal(local.clients[0].destroyed, true)
      assert.deepEqual(await first, retryable)
    } else {
      // Node keeps the admitted stream alive and must accept its real response.
      assert.equal(local.clients[0].destroyed, false)
      accept(retiring)
      assert.equal((await first).outcome, 'accepted')
    }
    // The old connection's asynchronous close must not evict its replacement.
    assert.equal((await local.send('still-replacement')).outcome, 'accepted')
    assert.equal(local.clients.length, 2)
  } finally {
    await local.close()
  }
})

test('APNs repeated GOAWAY has at most one accepting and one draining session per environment', async () => {
  const local = await fixture((stream) => stream.session?.goaway(constants.NGHTTP2_NO_ERROR, stream.id))
  try {
    const requests: ReturnType<typeof local.send>[] = []
    for (let i = 0; i < 6; i++) {
      requests.push(local.send(`retiring-${i}`))
      await once(local.clients[i], 'goaway')
      assert(local.clients.filter((client) => !client.destroyed).length <= 2)
    }
    local.network.close()
    assert.deepEqual(
      await Promise.all(requests),
      Array.from({ length: 6 }, () => retryable)
    )
    assert(local.clients.every((client) => client.destroyed))
  } finally {
    await local.close()
  }
})

test('APNs stream deadline preserves its healthy sibling and reconnects subsequent requests', async () => {
  let healthy: ServerHttp2Stream | undefined
  let signalHealthy: (() => void) | undefined
  const received = new Promise<void>((resolve) => {
    signalHealthy = resolve
  })
  const local = await fixture((stream, path) => {
    if (path.endsWith('/healthy')) {
      healthy = stream
      signalHealthy?.()
    } else if (!path.endsWith('/stalled')) {
      accept(stream)
    }
  }, 1000)
  try {
    const stalled = local.send('stalled')
    await new Promise((resolve) => setTimeout(resolve, 350))
    const sibling = local.send('healthy')
    await received
    assert.deepEqual(await stalled, retryable)
    assert(healthy)
    assert.equal(healthy.destroyed, false)
    accept(healthy)
    assert.equal((await local.send()).outcome, 'accepted')
    assert.equal((await sibling).outcome, 'accepted')
    assert.equal(local.clients.length, 2)
  } finally {
    await local.close()
  }
})

test('APNs shutdown promptly settles stalled streams in both environments and forbids reconnect', async () => {
  let seen = 0
  let allReceived: (() => void) | undefined
  const received = new Promise<void>((resolve) => {
    allReceived = resolve
  })
  const local = await fixture(() => {
    seen++
    if (seen === 2) {
      allReceived?.()
    }
  }, 60_000)
  try {
    const requests = [local.send('one'), local.send('two', 'production')]
    await received
    const started = Date.now()
    local.network.close()
    local.network.close()
    assert.deepEqual(await Promise.all(requests), [retryable, retryable])
    assert(Date.now() - started < 1000)
    assert(local.clients.every((client) => client.destroyed))
    assert.deepEqual(await local.send(), retryable)
    assert.equal(local.clients.length, 2)
  } finally {
    await local.close()
  }
})

test('APNs oversized response cancels one stream without discarding a healthy session', async () => {
  const local = await fixture((stream, path) => {
    if (path.endsWith('/oversized')) {
      stream.respond({ ':status': 500 })
      stream.end('x'.repeat(4097))
    } else {
      accept(stream)
    }
  })
  try {
    assert.deepEqual(await local.send('oversized'), retryable)
    assert.equal((await local.send()).outcome, 'accepted')
    assert.equal(local.clients.length, 1)
  } finally {
    await local.close()
  }
})

test('APNs missing or invalid final status can never cause permanent ciphertext removal', async () => {
  for (const status of [0, Number.NaN, Number.POSITIVE_INFINITY, -1, 100, 199, 200.5, 600]) {
    assert.deepEqual(classify({ status }), retryable)
    const transport = new ApnsTransport(
      config,
      async () => ({ status }),
      () => now
    )
    assert.deepEqual(await transport.send('device', 'sandbox', eventId, now + 1800, {}), retryable)
  }
})

test('APNs response classification remains intact over real HTTP/2', async () => {
  const local = await fixture((stream, path) => {
    const status = Number(path.split('/').at(-1))
    stream.respond({ ':status': status })
    stream.end(JSON.stringify({ reason: status === 410 ? 'Unregistered' : 'BadDeviceToken' }))
  })
  try {
    assert.deepEqual(await local.send('410'), { outcome: 'permanent', reason: 'Unregistered', device_invalid: true })
    assert.deepEqual(await local.send('400'), {
      outcome: 'permanent',
      reason: 'DeviceOrEnvironmentMismatch',
      device_invalid: false
    })
    for (const status of [429, 500, 503]) {
      assert.deepEqual(await local.send(String(status)), {
        outcome: 'retryable',
        reason: 'ProviderUnavailable',
        device_invalid: false
      })
    }
  } finally {
    await local.close()
  }
})

// A real TCP peer deliberately keeps its write half open after GOAWAY. An
// Http2Session reporting destroyed is insufficient evidence of socket cleanup.
function frame(type: number, flags: number, id: number, body: Buffer = Buffer.alloc(0)): Buffer {
  const header = Buffer.alloc(9)
  header.writeUIntBE(body.length, 0, 3)
  header[3] = type
  header[4] = flags
  header.writeUInt32BE(id, 5)
  return Buffer.concat([header, body])
}

function statusField(status: string): Buffer {
  return Buffer.concat([Buffer.from([8, status.length]), Buffer.from(status)])
}

async function rawPeer(status: string, responseFrames?: (id: number) => Buffer) {
  const peers = new Set<Socket>()
  const server = createTcpServer({ allowHalfOpen: true }, (socket) => {
    peers.add(socket)
    socket.on('error', () => {
      /* Cleanup may reset the deliberately half-open peer. */
    })
    socket.on('close', () => peers.delete(socket))
    let buffer = Buffer.alloc(0)
    let preface = false
    socket.on('data', (data: Buffer) => {
      buffer = Buffer.concat([buffer, data])
      if (!preface) {
        if (buffer.length < 24) {
          return
        }
        buffer = buffer.subarray(24)
        preface = true
        socket.write(frame(4, 0, 0)) // SETTINGS
      }
      while (buffer.length >= 9) {
        const length = buffer.readUIntBE(0, 3)
        if (buffer.length < 9 + length) {
          return
        }
        const type = buffer[3]
        const flags = buffer[4]
        const id = buffer.readUInt32BE(5)
        buffer = buffer.subarray(9 + length)
        if (type === 4 && flags % 2 === 0) {
          socket.write(frame(4, 1, 0)) // SETTINGS acknowledgement
        }
        if (type === 1) {
          // HPACK literal :status (indexed name 8); END_HEADERS | END_STREAM.
          socket.write(responseFrames ? responseFrames(id) : frame(1, 5, id, statusField(status)))
          const goaway = Buffer.alloc(8)
          goaway.writeUInt32BE(id, 0)
          socket.write(frame(7, 0, 0, goaway))
        }
      }
    })
  })
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  const address = server.address()
  assert(address && typeof address === 'object')
  return { address, server, peers }
}

test('APNs completed response before GOAWAY is accepted and its half-open socket closes', async () => {
  const { address, server, peers } = await rawPeer('200')
  const closed: Promise<unknown>[] = []
  const network = new AppleNetwork(() => {
    const client = connect(`http://127.0.0.1:${address.port}`)
    closed.push(once(client, 'close'))
    return client
  }, 1000)
  const transport = new ApnsTransport(config, network.send, () => now)
  let deadline: ReturnType<typeof setTimeout> | undefined
  try {
    await Promise.race([
      (async () => {
        for (let i = 0; i < 3; i++) {
          const reply = await transport.send('device', 'sandbox', eventId, now + 1800, {})
          assert.deepEqual(reply, { outcome: 'accepted', reason: 'Accepted', device_invalid: false })
          await closed[i]
        }
      })(),
      new Promise<never>((_, reject) => {
        deadline = setTimeout(() => reject(new Error('APNs socket was not destroyed')), 1500)
      })
    ])
    assert.equal(closed.length, 3)
  } finally {
    clearTimeout(deadline)
    network.close()
    for (const peer of peers) {
      peer.destroy()
    }
    await new Promise<void>((resolve) => server.close(() => resolve()))
  }
})

test('APNs connection failure settles competing sends and reconnects next time', async () => {
  const server = createTcpServer()
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  const address = server.address()
  assert(address && typeof address === 'object')
  await new Promise<void>((resolve) => server.close(() => resolve()))
  let opened = 0
  const network = new AppleNetwork(() => {
    opened++
    return connect(`http://127.0.0.1:${address.port}`)
  }, 100)
  try {
    const requests = Array.from({ length: 8 }, () =>
      network.send({
        environment: 'sandbox',
        token: 'device',
        headers: {},
        payload: '{}'
      })
    )
    assert((await Promise.allSettled(requests)).every((reply) => reply.status === 'rejected'))
    assert.equal(opened, 1)
    await assert.rejects(network.send({ environment: 'sandbox', token: 'device', headers: {}, payload: '{}' }))
    assert.equal(opened, 2)
  } finally {
    network.close()
  }
})

test('APNs aborted streams settle once even when error, end and close events follow', async () => {
  let settlements = 0
  const local = await fixture((stream) => stream.close(constants.NGHTTP2_CANCEL))
  try {
    const reply = await local.send().then((value) => {
      settlements++
      return value
    })
    assert.deepEqual(reply, retryable)
    // Allow all queued stream/session close and error callbacks to run.
    await new Promise<void>((resolve) => setImmediate(resolve))
    local.network.close()
    await new Promise<void>((resolve) => setImmediate(resolve))
    assert.equal(settlements, 1)
  } finally {
    await local.close()
  }
})

test('APNs malformed wire status is a retryable transport failure', async () => {
  for (const status of ['0', 'NaN', '200.5', '999']) {
    const { address, server, peers } = await rawPeer(status)
    const network = new AppleNetwork(() => connect(`http://127.0.0.1:${address.port}`), 200)
    const transport = new ApnsTransport(config, network.send, () => now)
    try {
      assert.deepEqual(await transport.send('device', 'sandbox', eventId, now + 1800, {}), retryable)
    } finally {
      network.close()
      for (const peer of peers) {
        peer.destroy()
      }
      await new Promise<void>((resolve) => server.close(() => resolve()))
    }
  }
})

test('APNs shutdown before connection establishment closes pending requests without orphan sockets', async () => {
  const local = await fixture(accept, 60_000)
  try {
    const request = local.send()
    const closed = once(local.clients[0], 'close')
    local.network.close()
    assert.deepEqual(await request, retryable)
    await closed
    assert.equal(local.clients.length, 1)
  } finally {
    await local.close()
  }
})

test('APNs clean GOAWAY without response headers is bounded by the stream deadline', async () => {
  let first = true
  const local = await fixture((stream) => {
    if (first) {
      first = false
      stream.session?.goaway(constants.NGHTTP2_NO_ERROR, stream.id)
    } else {
      accept(stream)
    }
  }, 100)
  try {
    assert.deepEqual(await local.send(), retryable)
    assert.equal((await local.send()).outcome, 'accepted')
    assert.equal(local.clients.length, 2)
  } finally {
    await local.close()
  }
})

for (const kind of ['duplicate', 'pseudo-trailer'] as const) {
  test(`APNs ${kind} status is rejected before an acceptance verdict`, async () => {
    const { address, server, peers } = await rawPeer('200', (id) =>
      kind === 'duplicate'
        ? frame(1, 5, id, Buffer.concat([statusField('200'), statusField('200.5')]))
        : Buffer.concat([frame(1, 4, id, statusField('200')), frame(1, 5, id, statusField('200.5'))])
    )
    const network = new AppleNetwork(() => connect(`http://127.0.0.1:${address.port}`), 200)
    const transport = new ApnsTransport(config, network.send, () => now)
    try {
      assert.deepEqual(await transport.send('device', 'sandbox', eventId, now + 1800, {}), retryable)
    } finally {
      network.close()
      for (const peer of peers) {
        peer.destroy()
      }
      await new Promise<void>((resolve) => server.close(() => resolve()))
    }
  })
}

for (const ending of ['end', 'reset'] as const) {
  test(`APNs 200 with a nonempty body is retryable even when followed by ${ending}`, async () => {
    const { address, server, peers } = await rawPeer('200', (id) =>
      Buffer.concat([
        frame(1, 4, id, statusField('200')),
        frame(0, ending === 'end' ? 1 : 0, id, Buffer.from('{"reason":')),
        ...(ending === 'reset' ? [frame(3, 0, id, Buffer.alloc(4))] : [])
      ])
    )
    const network = new AppleNetwork(() => connect(`http://127.0.0.1:${address.port}`), 200)
    const transport = new ApnsTransport(config, network.send, () => now)
    try {
      assert.deepEqual(await transport.send('device', 'sandbox', eventId, now + 1800, {}), retryable)
    } finally {
      network.close()
      for (const peer of peers) {
        peer.destroy()
      }
      await new Promise<void>((resolve) => server.close(() => resolve()))
    }
  })
}
