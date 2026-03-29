interface OrphanServerCandidate {
  name: string
  online: boolean
  os: string | null
}

const DEFAULT_SERVER_NAME = 'New Server'

export function isCleanupCandidate(server: OrphanServerCandidate): boolean {
  return server.name === DEFAULT_SERVER_NAME && !server.os && !server.online
}

export function countCleanupCandidates(servers: OrphanServerCandidate[]): number {
  return servers.filter(isCleanupCandidate).length
}
