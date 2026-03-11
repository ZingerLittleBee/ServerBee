type MessageHandler = (data: unknown) => void

const MIN_RECONNECT_DELAY = 1000
const MAX_RECONNECT_DELAY = 30_000
const JITTER_FACTOR = 0.2

export class WsClient {
  private ws: WebSocket | null = null
  private readonly handlers: Set<MessageHandler> = new Set()
  private reconnectDelay = MIN_RECONNECT_DELAY
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private closed = false
  private readonly url: string

  constructor(path: string) {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    this.url = `${protocol}//${window.location.host}${path}`
    this.connect()
  }

  private connect(): void {
    if (this.closed) {
      return
    }

    this.ws = new WebSocket(this.url)

    this.ws.onopen = () => {
      this.reconnectDelay = MIN_RECONNECT_DELAY
    }

    this.ws.onmessage = (event: MessageEvent) => {
      try {
        const data: unknown = JSON.parse(event.data as string)
        for (const handler of this.handlers) {
          handler(data)
        }
      } catch {
        // Ignore malformed messages
      }
    }

    this.ws.onclose = () => {
      this.scheduleReconnect()
    }

    this.ws.onerror = () => {
      this.ws?.close()
    }
  }

  private scheduleReconnect(): void {
    if (this.closed) {
      return
    }

    const jitter = 1 + (Math.random() * 2 - 1) * JITTER_FACTOR
    const delay = Math.min(this.reconnectDelay * jitter, MAX_RECONNECT_DELAY)

    this.reconnectTimer = setTimeout(() => {
      this.reconnectDelay = Math.min(this.reconnectDelay * 2, MAX_RECONNECT_DELAY)
      this.connect()
    }, delay)
  }

  onMessage(handler: MessageHandler): () => void {
    this.handlers.add(handler)
    return () => {
      this.handlers.delete(handler)
    }
  }

  close(): void {
    this.closed = true
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }
    this.ws?.close()
    this.ws = null
    this.handlers.clear()
  }
}
