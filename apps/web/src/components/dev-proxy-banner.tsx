function getDevProxyTarget() {
  const target = import.meta.env.VITE_DEV_PROXY_TARGET

  if (typeof target === 'string' && target.length > 0) {
    return target
  }

  return 'unknown'
}

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

export function DevProxyBanner() {
  const state = getDevProxyBannerState({
    allowWrites: Reflect.get(import.meta.env, 'VITE_DEV_PROXY_ALLOW_WRITES'),
    mode: import.meta.env.MODE,
    target: getDevProxyTarget()
  })

  if (!state) {
    return null
  }

  return (
    <div
      className={state.className}
      role="alert"
      style={{ left: 0, pointerEvents: 'none', position: 'fixed', right: 0, top: 0, zIndex: 2_147_483_647 }}
    >
      {state.message}
    </div>
  )
}
