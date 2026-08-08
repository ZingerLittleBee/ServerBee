/**
 * Named page-level boneyard skeletons for ServerBee's major loading surfaces.
 *
 * Each component renders the loading branch of one surface: a `BoneSkeleton`
 * whose `name` resolves bones from the generated registry (`@/bones`), sized
 * by the matching capture fixture as hidden children. Names are deterministic
 * and mirrored one-to-one on the `/boneyard-capture` route — renaming here
 * means renaming there and regenerating bones (`bun run generate:bones`).
 *
 * The two detail skeletons share `ServerDetailFixture` because the public
 * status detail and the authed detail render the same `ServerDetailContent`
 * layout; they stay separate names because their page containers differ
 * (status max-w-6xl vs authed sidebar inset), so each needs its own capture.
 */
import { BoneSkeleton } from './bone-skeleton'
import {
  ServerDetailFixture,
  ServiceMonitorDetailFixture,
  StatusNetworkFixture,
  StatusOverviewFixture,
  TrafficOverviewFixture
} from './fixtures'

/** /status — public status overview grid. */
export function StatusOverviewSkeleton() {
  return (
    <BoneSkeleton loading name="status-overview">
      <StatusOverviewFixture />
    </BoneSkeleton>
  )
}

/** /status/server/$serverId — public server detail. */
export function StatusServerDetailSkeleton() {
  return (
    <BoneSkeleton loading name="status-server-detail">
      <ServerDetailFixture variant="public" />
    </BoneSkeleton>
  )
}

/** /status/network/$serverId — standalone public network detail. */
export function StatusNetworkSkeleton() {
  return (
    <BoneSkeleton loading name="status-network-detail">
      <StatusNetworkFixture />
    </BoneSkeleton>
  )
}

/** /servers/$id — authed server detail (query loading and lazy-page fallback). */
export function ServerDetailSkeleton() {
  return (
    <BoneSkeleton loading name="server-detail">
      <ServerDetailFixture variant="admin" />
    </BoneSkeleton>
  )
}

/** /traffic — authed traffic overview. */
export function TrafficOverviewSkeleton() {
  return (
    <BoneSkeleton loading name="traffic-overview">
      <TrafficOverviewFixture />
    </BoneSkeleton>
  )
}

/** /service-monitors/$id — authed service monitor detail. */
export function ServiceMonitorDetailSkeleton() {
  return (
    <BoneSkeleton loading name="service-monitor-detail">
      <ServiceMonitorDetailFixture />
    </BoneSkeleton>
  )
}
