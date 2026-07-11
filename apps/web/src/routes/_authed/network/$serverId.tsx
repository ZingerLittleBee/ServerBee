import { createFileRoute, redirect } from '@tanstack/react-router'
import { legacyNetworkRangeToRangeKey } from '@/lib/server-detail-nav'

// The standalone network detail page merged into the server detail Network
// tab. This route survives purely as a redirect so old bookmarks and links
// keep working.
export const Route = createFileRoute('/_authed/network/$serverId')({
  validateSearch: (search: Record<string, unknown>) => ({
    range: (search.range as string) || 'realtime'
  }),
  beforeLoad: ({ params, search }) => {
    throw redirect({
      to: '/servers/$id',
      params: { id: params.serverId },
      search: { tab: 'network', range: legacyNetworkRangeToRangeKey(search.range) },
      replace: true
    })
  }
})
