import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft } from 'lucide-react'
import { useCallback, useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { TerminalView } from '@/components/terminal/terminal-view'
import { Button } from '@/components/ui/button'
import { useTerminalWs } from '@/hooks/use-terminal-ws'

export const Route = createFileRoute('/_authed/terminal/$serverId')({
  component: TerminalPage
})

function statusColor(status: string): string {
  if (status === 'connected') {
    return 'bg-green-500'
  }
  if (status === 'connecting') {
    return 'bg-yellow-500'
  }
  return 'bg-red-500'
}

function statusLabel(status: string, t: (key: string) => string): string {
  if (status === 'connected') {
    return t('status_connected')
  }
  if (status === 'connecting') {
    return t('status_connecting')
  }
  return t('status_closed')
}

function TerminalPage() {
  const { t } = useTranslation('terminal')
  const { serverId } = Route.useParams()
  const { connect, disconnect, error, onData, sendInput, sendResize, status } = useTerminalWs(serverId)
  const writeRef = useRef<((data: string) => void) | null>(null)

  // Forward terminal output from WS to xterm
  useEffect(() => {
    onData((data) => {
      if (writeRef.current) {
        writeRef.current(data)
      }
    })
  }, [onData])

  // Auto-connect on mount
  useEffect(() => {
    connect()
    return () => {
      disconnect()
    }
  }, [connect, disconnect])

  const handleData = useCallback(
    (data: string) => {
      sendInput(data)
    },
    [sendInput]
  )

  const handleResize = useCallback(
    (rows: number, cols: number) => {
      sendResize(rows, cols)
    },
    [sendResize]
  )

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b px-4 py-2">
        <Link params={{ id: serverId }} search={{}} to="/servers/$id">
          <Button size="sm" variant="ghost">
            <ArrowLeft className="size-4" />
            {t('back')}
          </Button>
        </Link>
        <h1 className="font-semibold text-lg">{t('title')}</h1>
        <span className="text-muted-foreground text-sm">{serverId.slice(0, 8)}...</span>
        <div className="ml-auto flex items-center gap-2">
          <span className={`inline-block size-2 rounded-full ${statusColor(status)}`} />
          <span className="text-muted-foreground text-xs">{statusLabel(status, t)}</span>
          {error && <span className="text-red-500 text-xs">{error}</span>}
          {status === 'closed' && (
            <Button onClick={connect} size="sm" variant="outline">
              {t('reconnect')}
            </Button>
          )}
        </div>
      </div>
      <div className="flex-1 p-2">
        <TerminalView onData={handleData} onResize={handleResize} writeRef={writeRef} />
      </div>
    </div>
  )
}
