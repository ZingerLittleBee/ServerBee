import type { ReactNode } from 'react'
import { BoneSkeleton } from '@/components/boneyard/bone-skeleton'
import {
  ServerDetailFixture,
  ServiceMonitorDetailFixture,
  StatusNetworkFixture,
  StatusOverviewGridFixture,
  StatusOverviewListFixture,
  TrafficOverviewFixture
} from '@/components/boneyard/fixtures'

/**
 * Capture surface for `boneyard-js build` (run via `bun run generate:bones`).
 * Renders every named page skeleton with its deterministic fixture so the CLI
 * can snapshot bone geometry without a backend, credentials, or production
 * data.
 *
 * This is deliberately NOT a file route: it is mounted by a dev-only branch
 * in the root layout (see routes/__root.tsx) when the pathname is
 * /boneyard-capture, so the production route tree never registers it.
 *
 * Width fidelity matters: bones are stored as percentages of each skeleton's
 * container and looked up by viewport width, so each fixture is wrapped in a
 * replica of its real layout container —
 * - public status pages: `mx-auto max-w-6xl px-4` (see routes/status.tsx)
 * - authed pages: 16rem sidebar offset at md+ plus `p-3 sm:p-4` content
 *   padding (see routes/_authed.tsx)
 */
export function BoneyardCapturePage() {
  return (
    <div className="min-h-full bg-background text-foreground">
      {/* Public status layout replica (routes/status.tsx) */}
      <div className="mx-auto max-w-6xl px-4 py-8">
        <CaptureEntry name="status-overview-grid">
          <StatusOverviewGridFixture />
        </CaptureEntry>
        <div aria-hidden="true" className="h-16" />
        <CaptureEntry name="status-overview-list">
          <StatusOverviewListFixture />
        </CaptureEntry>
        <div aria-hidden="true" className="h-16" />
        <CaptureEntry name="status-server-detail">
          <ServerDetailFixture variant="public" />
        </CaptureEntry>
        <div aria-hidden="true" className="h-16" />
        <CaptureEntry name="status-network-detail">
          <StatusNetworkFixture />
        </CaptureEntry>
      </div>

      {/* Authed sidebar-inset layout replica (routes/_authed.tsx) */}
      <div className="md:pl-64">
        <div className="p-3 sm:p-4">
          <CaptureEntry name="server-detail">
            <ServerDetailFixture variant="admin" />
          </CaptureEntry>
          <div aria-hidden="true" className="h-16" />
          <CaptureEntry name="traffic-overview">
            <TrafficOverviewFixture />
          </CaptureEntry>
          <div aria-hidden="true" className="h-16" />
          <CaptureEntry name="service-monitor-detail">
            <ServiceMonitorDetailFixture />
          </CaptureEntry>
        </div>
      </div>
    </div>
  )
}

/**
 * One named capture target. The `fixture` prop is what the CLI snapshots in
 * build mode; the same markup is passed as children so the surface stays
 * inspectable when opened in a normal dev session.
 */
function CaptureEntry({ children, name }: { children: ReactNode; name: string }) {
  return (
    <BoneSkeleton fixture={children} loading name={name}>
      {children}
    </BoneSkeleton>
  )
}
