import type { AgentAuthorityStateSummary } from '@/lib/api-schema'

export type StatusKind = 'online' | 'offline' | 'pending'

export function deriveServerStatus(s: {
  agent_authority?: AgentAuthorityStateSummary
  has_token?: boolean
  online: boolean
}): StatusKind {
  const unclaimed = s.agent_authority ? s.agent_authority.status === 'unclaimed' : s.has_token === false
  if (unclaimed) {
    return 'pending'
  }
  return s.online ? 'online' : 'offline'
}
