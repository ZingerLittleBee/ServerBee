import { useMutation, useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { ChevronDown, ChevronRight, Play, Terminal } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { api } from '@/lib/api-client'
import type { TaskResponse, TaskResult } from '@/lib/api-schema'

export const Route = createFileRoute('/_authed/settings/tasks')({
  component: TasksPage
})

interface ServerInfo {
  capabilities?: number
  id: string
  name: string
}

function TasksPage() {
  const { t } = useTranslation(['settings', 'common'])
  const [command, setCommand] = useState('')
  const [selectedServerIds, setSelectedServerIds] = useState<string[]>([])
  const [timeout, setTimeout] = useState(30)
  const [expandedTask, setExpandedTask] = useState<string | null>(null)

  const { data: servers } = useQuery<ServerInfo[]>({
    queryKey: ['servers-list'],
    queryFn: () => api.get<ServerInfo[]>('/api/servers')
  })

  const createMutation = useMutation({
    mutationFn: (input: { command: string; server_ids: string[]; timeout: number }) =>
      api.post<TaskResponse>('/api/tasks', input),
    onSuccess: (data) => {
      setExpandedTask(data.id)
    }
  })

  const { data: taskResults, refetch: refetchResults } = useQuery<TaskResult[]>({
    queryKey: ['task-results', expandedTask],
    queryFn: () => api.get<TaskResult[]>(`/api/tasks/${expandedTask}/results`),
    enabled: expandedTask !== null,
    refetchInterval: expandedTask ? 2000 : false
  })

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (command.trim().length === 0 || selectedServerIds.length === 0) {
      return
    }
    createMutation.mutate({
      command: command.trim(),
      server_ids: selectedServerIds,
      timeout
    })
  }

  const toggleServer = (id: string) => {
    setSelectedServerIds((prev) => (prev.includes(id) ? prev.filter((s) => s !== id) : [...prev, id]))
  }

  const selectAll = () => {
    if (!servers) {
      return
    }
    setSelectedServerIds(selectedServerIds.length === servers.length ? [] : servers.map((s) => s.id))
  }

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">{t('tasks.title')}</h1>

      <div className="max-w-3xl space-y-6">
        {/* Command form */}
        <div className="rounded-lg border bg-card p-6">
          <h2 className="mb-4 font-semibold text-lg">{t('tasks.execute')}</h2>

          <form className="space-y-4" onSubmit={handleSubmit}>
            <div>
              <label className="mb-1 block font-medium text-sm" htmlFor="command-input">
                {t('tasks.command')}
              </label>
              <div className="flex gap-2">
                <div className="relative flex-1">
                  <Terminal className="absolute top-2.5 left-3 size-4 text-muted-foreground" />
                  <input
                    className="flex h-9 w-full rounded-md border border-input bg-transparent py-1 pr-3 pl-9 font-mono text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                    id="command-input"
                    onChange={(e) => setCommand(e.target.value)}
                    placeholder={t('tasks.command_placeholder')}
                    required
                    type="text"
                    value={command}
                  />
                </div>
                <input
                  className="flex h-9 w-20 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
                  min={1}
                  onChange={(e) => setTimeout(Number.parseInt(e.target.value, 10) || 30)}
                  title={t('tasks.timeout')}
                  type="number"
                  value={timeout}
                />
              </div>
            </div>

            <div>
              <div className="mb-2 flex items-center justify-between">
                <span className="font-medium text-sm">{t('tasks.target_servers')}</span>
                <Button onClick={selectAll} size="sm" type="button" variant="ghost">
                  {servers && selectedServerIds.length === servers.length
                    ? t('tasks.deselect_all')
                    : t('tasks.select_all')}
                </Button>
              </div>
              {!servers || servers.length === 0 ? (
                <p className="text-muted-foreground text-sm">{t('tasks.no_servers')}</p>
              ) : (
                <div className="grid grid-cols-2 gap-1">
                  {servers.map((srv) => {
                    // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask check
                    const execEnabled = !srv.capabilities || (srv.capabilities & 2) !== 0
                    return (
                      <label
                        className={`flex cursor-pointer items-center gap-2 rounded-md border px-3 py-2 text-sm transition-colors has-[:checked]:border-primary has-[:checked]:bg-primary/5 ${
                          execEnabled ? '' : 'cursor-not-allowed opacity-50'
                        }`}
                        key={srv.id}
                        title={execEnabled ? undefined : t('tasks.exec_disabled')}
                      >
                        <input
                          checked={selectedServerIds.includes(srv.id)}
                          disabled={!execEnabled}
                          onChange={() => toggleServer(srv.id)}
                          type="checkbox"
                        />
                        {srv.name}
                      </label>
                    )
                  })}
                </div>
              )}
            </div>

            <Button
              disabled={createMutation.isPending || selectedServerIds.length === 0 || command.trim().length === 0}
              type="submit"
            >
              <Play className="size-4" />
              {t('tasks.execute_count', {
                count: selectedServerIds.length,
                plural: selectedServerIds.length !== 1 ? 's' : ''
              })}
            </Button>
          </form>
        </div>

        {/* Results */}
        {createMutation.data && (
          <div className="rounded-lg border bg-card">
            <button
              className="flex w-full items-center justify-between px-6 py-4 text-left"
              onClick={() => {
                setExpandedTask((prev) => (prev === createMutation.data.id ? null : createMutation.data.id))
                refetchResults().catch(() => undefined)
              }}
              type="button"
            >
              <div>
                <p className="font-medium text-sm">
                  <code>{createMutation.data.command}</code>
                </p>
                <p className="text-muted-foreground text-xs">
                  {new Date(createMutation.data.created_at).toLocaleString()} | {createMutation.data.server_ids.length}{' '}
                  server(s)
                </p>
              </div>
              {expandedTask === createMutation.data.id ? (
                <ChevronDown className="size-4" />
              ) : (
                <ChevronRight className="size-4" />
              )}
            </button>

            {expandedTask === createMutation.data.id && (
              <div className="border-t">
                {!taskResults || taskResults.length === 0 ? (
                  <div className="flex items-center gap-2 px-6 py-4 text-muted-foreground text-sm">
                    <div className="size-3 animate-spin rounded-full border-2 border-current border-t-transparent" />
                    {t('tasks.waiting')}
                  </div>
                ) : (
                  <div className="divide-y">
                    {taskResults.map((result) => {
                      const serverName = servers?.find((s) => s.id === result.server_id)?.name ?? result.server_id
                      const isSkipped = result.exit_code === -2
                      return (
                        <div className="px-6 py-3" key={result.id}>
                          <div className="mb-1 flex items-center gap-2">
                            <span className="font-medium text-sm">{serverName}</span>
                            {isSkipped ? (
                              <span className="rounded bg-muted px-1.5 py-0.5 text-muted-foreground text-xs">
                                {t('tasks.skipped')}
                              </span>
                            ) : (
                              <span
                                className={`rounded px-1.5 py-0.5 text-xs ${
                                  result.exit_code === 0
                                    ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
                                    : 'bg-red-500/10 text-red-600 dark:text-red-400'
                                }`}
                              >
                                {t('tasks.exit_code', { code: result.exit_code })}
                              </span>
                            )}
                          </div>
                          {isSkipped ? (
                            <p className="text-muted-foreground text-xs italic">{t('tasks.exec_disabled')}</p>
                          ) : (
                            <pre className="max-h-40 overflow-auto whitespace-pre-wrap rounded-md bg-muted/50 p-2 font-mono text-xs">
                              {result.output || t('tasks.no_output')}
                            </pre>
                          )}
                        </div>
                      )
                    })}
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
