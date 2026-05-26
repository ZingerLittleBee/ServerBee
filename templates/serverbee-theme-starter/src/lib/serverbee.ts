export interface Server {
  id: string
  name: string
}

async function get<T>(path: string): Promise<T> {
  const res = await fetch(path, { credentials: 'include' })
  if (!res.ok) {
    throw new Error(`${res.status} ${res.statusText}`)
  }
  const j = await res.json()
  return j.data as T
}

export const fetchServers = () => get<Server[]>('/api/servers')
