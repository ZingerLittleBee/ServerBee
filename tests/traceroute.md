# Traceroute Manual E2E Checklist

Run through this list after deploying changes to the agent + server. Repeat once per supported platform if possible.

## Setup

- Agent built from this branch deployed to a test VPS (Linux + macOS recommended).
- Either run agent as root, or apply: `sudo setcap cap_net_raw+ep $(which serverbee-agent)`.
- Server running with the new migration applied.
- Two browser sessions: one admin, one member.

## Happy path (ICMP)

- [ ] Admin: open network detail page → click "路由追踪" → enter `1.1.1.1` → protocol ICMP → Run
- [ ] Verify the table fills hop-by-hop within ~5 seconds; round counter goes 1→5
- [ ] Final hop shows non-zero `total_recv`, valid `avg_ms`, hostname populated where reverse DNS works
- [ ] After completion, a new entry appears in the history list with `icmp` chip
- [ ] Close dialog and reopen → newest record is the one we just ran

## UDP

- [ ] Admin: trigger trace to `1.1.1.1` with protocol UDP
- [ ] Verify it completes without `BadConfig` (regression for the PortDirection fix)
- [ ] Hops mostly populated; some intermediate hops may show ICMP TimeExceeded fine

## TCP

- [ ] Admin: trigger trace to `1.1.1.1` with protocol TCP
- [ ] Verify completion; route may differ from ICMP due to load balancers

## Privilege fallback

- [ ] Linux: stop the agent, remove CAP_NET_RAW (`sudo setcap -r $(which serverbee-agent)`),
      restart agent as non-root
- [ ] Trigger any trace; verify the error toast contains the setcap one-liner
- [ ] Re-apply setcap; verify next trace succeeds without restarting the agent

## ECMP / multi-IP

- [ ] Trace to a target known to use ECMP (e.g., `cloudflare.com`)
- [ ] Some hops show `+N` chip; hover reveals the alternate IPs

## History + admin gating

- [ ] Member account: open the dialog
- [ ] Verify no Run form is visible; instead see the read-only note
- [ ] History list is visible; clicking a row shows the snapshot in the table
- [ ] No trash icons, no Clear all button
- [ ] Admin: delete a single record → list shrinks
- [ ] Admin: Clear all → confirm dialog → list empties

## WebSocket reconnect / refresh

- [ ] Admin: trigger a long trace (e.g., target with high TTL like a route to Australia from US)
- [ ] Mid-flight, hard-refresh the page → reopen the dialog → snapshot continues to update via the GET fallback
- [ ] Confirm completion still records to history

## Stale-meta drop

- [ ] Trigger a trace and immediately delete the cache by restarting the server (history is preserved in DB)
- [ ] Verify the running trace no longer streams to the UI (cache evicted) but completed result lands in DB

## Capability denied

- [ ] On the server side, disable `CAP_PING_ICMP` for the test server
- [ ] Trigger a trace → result shows the capability-denied error immediately, no agent activity
