# Network quality detail lives in the server detail Network tab

The standalone network detail pages were merged into the server detail page as a Network tab (admin: `/servers/$id?tab=network`, public: `/status/server/$serverId?tab=network`), sharing the server detail `range` search param (metrics-style keys). `/network/$serverId` is now an unconditional redirect kept only for old links.

The public side keeps `/status/network/$serverId` as a **conditional fallback** rather than an unconditional redirect: status pages have two independent config toggles, `show_server_detail` and `show_network`. A site configured with `show_server_detail=false` but `show_network=true` has no server detail page to host the tab, so the standalone public network page must keep rendering there — redirecting unconditionally would strand its network data on a `/status` bounce. When `show_server_detail` is enabled, the standalone page redirects into the tab and the network overview cards deep-link to the tab directly, so the network detail has a single canonical home whenever one is possible.

Considered and rejected: (a) keeping both admin entry points alive — duplicate UI to maintain and ambiguous URL semantics for the same data; (b) collapsing the public toggles so `show_server_detail=false` also hides network detail — silently changes the meaning of existing deployments' configs.

In-flight traceroute run state (request id, latest stream frame, selection) is hoisted into `stores/traceroute-store.ts` keyed by server id, because the tab unmounts on tab switch and each `traceroute_update` WS frame carries full state — persisting the last frame makes resume free, with no reconnect logic.
