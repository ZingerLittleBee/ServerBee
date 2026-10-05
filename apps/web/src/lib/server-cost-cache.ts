import type { QueryClient } from '@tanstack/react-query'

export function invalidateServerCosts(queryClient: QueryClient, serverIds?: readonly string[]): void {
  queryClient.invalidateQueries({ queryKey: ['cost', 'overview'] }).catch(() => undefined)
  queryClient
    .invalidateQueries({
      predicate: ({ queryKey }) =>
        queryKey[0] === 'servers' &&
        queryKey[2] === 'cost-insights' &&
        (serverIds === undefined || serverIds.includes(queryKey[1] as string))
    })
    .catch(() => undefined)
}
