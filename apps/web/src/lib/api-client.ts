class ApiError extends Error {
  status: number

  constructor(message: string, status: number) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const options: RequestInit = {
    method,
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json'
    }
  }

  if (body !== undefined) {
    options.body = JSON.stringify(body)
  }

  const response = await fetch(path, options)

  if (!response.ok) {
    const text = await response.text().catch(() => response.statusText)
    throw new ApiError(text, response.status)
  }

  if (response.status === 204) {
    return undefined as T
  }

  const json = await response.json()
  if (json && typeof json === 'object' && 'data' in json) {
    return json.data as T
  }
  return json as T
}

export const api = {
  get: <T>(path: string) => request<T>('GET', path),
  post: <T>(path: string, body?: unknown) => request<T>('POST', path, body),
  put: <T>(path: string, body?: unknown) => request<T>('PUT', path, body),
  delete: <T>(path: string) => request<T>('DELETE', path)
}

export { ApiError }
