function getDevProxyWritesEnabled(value: unknown) {
  return value === '1'
}

export function getDevProxyBannerState({
  allowWrites,
  mode,
  target
}: {
  allowWrites: unknown
  mode: string
  target: unknown
}) {
  if (mode !== 'prod-proxy') {
    return null
  }

  if (getDevProxyWritesEnabled(allowWrites)) {
    return {
      className:
        'border-b border-red-950 bg-red-700 px-4 py-2 text-center font-semibold text-white text-xs tracking-[0.08em] shadow-lg',
      message: `⚠ Dev proxy → PROD (${typeof target === 'string' && target.length > 0 ? target : 'unknown'}) · WRITE ACCESS ENABLED`
    }
  }

  return {
    className:
      'border-orange-950 border-b bg-orange-500 px-4 py-2 text-center font-semibold text-black text-xs tracking-[0.08em] shadow-lg',
    message: `⚠ Dev proxy → PROD (${typeof target === 'string' && target.length > 0 ? target : 'unknown'}) · read-only`
  }
}
