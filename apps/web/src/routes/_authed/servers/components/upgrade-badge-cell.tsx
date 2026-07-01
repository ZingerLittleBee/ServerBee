import { UpgradeJobBadge } from '@/components/server/upgrade-job-badge'
import { useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'

export function UpgradeBadgeCell({ serverId }: { serverId: string }) {
  const job = useUpgradeJobsStore((state) => state.jobs.get(serverId))
  return <UpgradeJobBadge job={job} />
}
