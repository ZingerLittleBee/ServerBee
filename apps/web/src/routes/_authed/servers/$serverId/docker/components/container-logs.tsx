import { Circle, Trash2 } from 'lucide-react'
import { useEffect, useRef } from 'react'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { cn } from '@/lib/utils'
import { useDockerLogs } from '../hooks/use-docker-logs'

interface ContainerLogsProps {
  containerId: string
  serverId: string
}

export function ContainerLogs({ serverId, containerId }: ContainerLogsProps) {
  const scrollRef = useRef<HTMLDivElement>(null)
  const followRef = useRef(true)

  const { logs, isConnected, start, stop, clearLogs } = useDockerLogs({
    serverId,
    containerId,
    follow: true,
    tail: 100
  })

  // Auto-start on mount
  useEffect(() => {
    start()
    return () => {
      stop()
    }
  }, [start, stop])

  // Auto-scroll when following and new logs arrive
  const logCount = logs.length
  useEffect(() => {
    if (logCount > 0 && followRef.current && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [logCount])

  const handleFollowChange = (checked: boolean) => {
    followRef.current = checked
    if (checked && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h4 className="font-medium text-sm">Logs</h4>
          <div className="flex items-center gap-1.5">
            <Circle
              aria-hidden="true"
              className={cn('size-2', isConnected ? 'fill-green-500 text-green-500' : 'fill-muted text-muted')}
            />
            <span className="text-muted-foreground text-xs">{isConnected ? 'Connected' : 'Disconnected'}</span>
          </div>
        </div>
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2">
            <Switch defaultChecked onCheckedChange={handleFollowChange} size="sm" />
            <Label className="text-xs">Follow</Label>
          </div>
          <Button onClick={clearLogs} size="sm" variant="ghost">
            <Trash2 aria-hidden="true" className="size-3.5" />
            Clear
          </Button>
        </div>
      </div>

      <div className="h-[300px] overflow-y-auto rounded-lg border bg-muted/30 p-3 font-mono text-xs" ref={scrollRef}>
        {logs.length === 0 ? (
          <p className="text-muted-foreground">No log entries yet...</p>
        ) : (
          <pre className="whitespace-pre-wrap break-all">
            {logs.map((entry) => (
              <code
                className={cn('block leading-relaxed', entry.stream === 'stderr' && 'text-red-500 dark:text-red-400')}
                key={`${entry.timestamp ?? ''}-${entry.stream}-${entry.message.slice(0, 40)}`}
              >
                {entry.timestamp && <span className="mr-2 text-muted-foreground">{entry.timestamp}</span>}
                {entry.message}
              </code>
            ))}
          </pre>
        )}
      </div>
    </div>
  )
}
