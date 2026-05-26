import { useEffect, useState } from 'react'
import { fetchServers, type Server } from './lib/serverbee'

export function App() {
  const [servers, setServers] = useState<Server[]>([])
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    fetchServers()
      .then(setServers)
      .catch((e) => setError(String(e)))
  }, [])
  if (error) {
    return <pre style={{ color: 'red' }}>{error}</pre>
  }
  return (
    <div style={{ fontFamily: 'system-ui', padding: 16 }}>
      <h1>Starter Theme</h1>
      <ul>
        {servers.map((s) => (
          <li key={s.id}>{s.name}</li>
        ))}
      </ul>
    </div>
  )
}
