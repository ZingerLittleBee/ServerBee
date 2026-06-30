import { getDevProxyBannerState } from './dev-proxy-banner-state'

function getDevProxyTarget() {
  const target = import.meta.env.VITE_DEV_PROXY_TARGET

  if (typeof target === 'string' && target.length > 0) {
    return target
  }

  return 'unknown'
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
