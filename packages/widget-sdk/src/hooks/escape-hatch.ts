import { type UseMutationResult, type UseQueryResult, useMutation, useQuery } from '@tanstack/react-query'

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(path, {
    method,
    credentials: 'include',
    headers: body ? { 'content-type': 'application/json' } : undefined,
    body: body ? JSON.stringify(body) : undefined
  })
  if (!res.ok) {
    throw new Error(`${method} ${path}: ${res.status}`)
  }
  const json = await res.json()
  return json && typeof json === 'object' && 'data' in json ? (json as { data: T }).data : (json as T)
}

export function useApiQuery<T>(
  path: string,
  opts?: {
    params?: Record<string, string | number | undefined>
    enabled?: boolean
  }
): UseQueryResult<T> {
  const params = opts?.params
  const url = params
    ? `${path}?${new URLSearchParams(
        Object.fromEntries(
          Object.entries(params)
            .filter(([, v]) => v !== undefined)
            .map(([k, v]) => [k, String(v)])
        )
      ).toString()}`
    : path
  return useQuery<T>({
    queryKey: ['widget-api', url],
    queryFn: () => request<T>('GET', url),
    enabled: opts?.enabled
  })
}

export function useApiMutation<TRes, TReq = void>(method: string, path: string): UseMutationResult<TRes, Error, TReq> {
  return useMutation<TRes, Error, TReq>({
    mutationFn: (body) => request<TRes>(method, path, body)
  })
}
