export type StatusKind = 'online' | 'offline' | 'pending'

export function deriveServerStatus(s: { has_token?: boolean; online: boolean }): StatusKind {
  if (s.has_token === false) {
    return 'pending'
  }
  return s.online ? 'online' : 'offline'
}
