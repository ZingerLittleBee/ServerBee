export function buildEmailPayload(from: string, toAddresses: string[]): { from: string; to: string[] } {
  return { from, to: toAddresses }
}
