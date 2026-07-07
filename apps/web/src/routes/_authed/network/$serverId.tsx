import { createFileRoute, redirect } from '@tanstack/react-router'

// The standalone network detail page merged into the server detail Network
// tab. This route survives purely as a redirect so old bookmarks and links
// keep working. The old `range` param counted hours ('1' | '6' | ... );
// the server detail page uses metrics-style keys.
const HOURS_TO_RANGE_KEY: Record<string, string> = {
  realtime: 'realtime',
  '1': '1h',
  '6': '6h',
  '24': '24h',
  '168': '7d',
  '720': '30d'
}

export const Route = createFileRoute('/_authed/network/$serverId')({
  validateSearch: (search: Record<string, unknown>) => ({
    range: (search.range as string) || 'realtime'
  }),
  beforeLoad: ({ params, search }) => {
    throw redirect({
      to: '/servers/$id',
      params: { id: params.serverId },
      search: { tab: 'network', range: HOURS_TO_RANGE_KEY[search.range] ?? 'realtime' },
      replace: true
    })
  }
})
