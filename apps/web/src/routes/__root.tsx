import { createRootRoute, Outlet } from '@tanstack/react-router'
import { Agentation } from 'agentation'
import { lazy, Suspense } from 'react'
import { DevProxyBanner } from '@/components/dev-proxy-banner'
import { ThemeProvider } from '@/components/theme-provider'
import { Toaster } from '@/components/ui/sonner'
import { TooltipProvider } from '@/components/ui/tooltip'

// Dev-only boneyard capture surface, mounted outside the router so the
// production route tree never registers /boneyard-capture. The lazy import
// keeps fixtures and fake data out of the production bundle's initial chunk;
// the import.meta.env.DEV gate makes the branch unreachable in prod builds.
const BoneyardCapturePage = lazy(() =>
  import('@/components/boneyard/capture-page').then((m) => ({ default: m.BoneyardCapturePage }))
)

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
