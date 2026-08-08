import { createRootRoute, Outlet } from '@tanstack/react-router'
import { Agentation } from 'agentation'
import { lazy, Suspense } from 'react'
import { DevProxyBanner } from '@/components/dev-proxy-banner'
import { ThemeProvider } from '@/components/theme-provider'
import { Toaster } from '@/components/ui/sonner'
import { TooltipProvider } from '@/components/ui/tooltip'

// Dev-only boneyard capture surface, mounted outside the router so the
// production route tree never registers /boneyard-capture. The import
// expression itself sits behind the compile-time import.meta.env.DEV gate:
// Vite replaces the flag with `false` in prod builds, so Rollup drops the
// dead branch and the capture-page chunk never ships.
const BoneyardCapturePage = import.meta.env.DEV
  ? lazy(() => import('@/components/boneyard/capture-page').then((m) => ({ default: m.BoneyardCapturePage })))
  : () => null

export const Route = createRootRoute({
  component: RootLayout
})

function RootLayout() {
  const isBoneyardCapture = import.meta.env.DEV && window.location.pathname === '/boneyard-capture'

  return (
    <>
      {import.meta.env.DEV && <Agentation />}
      <DevProxyBanner />
      <ThemeProvider>
        <TooltipProvider>
          <div className="h-dvh overflow-hidden bg-background text-foreground">
            {isBoneyardCapture ? (
              <Suspense fallback={null}>
                <BoneyardCapturePage />
              </Suspense>
            ) : (
              <Outlet />
            )}
          </div>
          <Toaster />
        </TooltipProvider>
      </ThemeProvider>
    </>
  )
}
