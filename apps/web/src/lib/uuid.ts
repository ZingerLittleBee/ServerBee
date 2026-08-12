/**
 * UUID v4 that also works outside secure contexts.
 *
 * `crypto.randomUUID` is only exposed on HTTPS/localhost origins; self-hosted
 * dashboards are routinely served over plain HTTP on a LAN or VPS IP, where
 * calling it throws. `crypto.getRandomValues` has no such restriction, so fall
 * back to assembling the UUID manually from it.
 */
export function randomUUID(): string {
  if (typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID()
  }
  const bytes = crypto.getRandomValues(new Uint8Array(16))
  // Per RFC 4122 §4.4: version 4, variant 10xx.
  // biome-ignore lint/suspicious/noBitwiseOperators: RFC 4122 version bits require masking
  bytes[6] = (bytes[6] & 0x0f) | 0x40
  // biome-ignore lint/suspicious/noBitwiseOperators: RFC 4122 variant bits require masking
  bytes[8] = (bytes[8] & 0x3f) | 0x80
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, '0'))
  return `${hex.slice(0, 4).join('')}-${hex.slice(4, 6).join('')}-${hex.slice(6, 8).join('')}-${hex.slice(8, 10).join('')}-${hex.slice(10).join('')}`
}
