import type { QueryClient, QueryFilters } from '@tanstack/react-query'

export function invalidateServerCosts(queryClient: QueryClient, serverIds?: readonly string[]): void {
  const filters: QueryFilters = {
    predicate: ({ queryKey }) =>
      (queryKey[0] === 'cost' && queryKey[1] === 'overview') ||
      (queryKey[0] === 'servers' &&
        queryKey[2] === 'cost-insights' &&
        (serverIds === undefined || serverIds.includes(queryKey[1] as string)))
  }
  // Invalidation alone reuses an initial pending request without cached data.
  // Cancel it first so advisories are fetched after the catalog change.
  queryClient
    .cancelQueries(filters)
    .then(() => queryClient.invalidateQueries(filters))
    .catch(() => undefined)
}
