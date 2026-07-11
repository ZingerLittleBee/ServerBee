# Ubiquitous Language

- **Network quality detail (网络质量详情)** — the per-server view of probe results (latency/loss per target, anomalies, traceroute). Since ADR-0001 it lives in the server detail **Network tab**; there is no separate admin page.
- **Network tab** — the server-detail tab hosting the network quality detail. Admin gets the full experience (chart, traceroute, target management, CSV export); the public status variant is the redacted summary (targets + anomalies only).
- **Server detail tabs** — the top-level structure of the server detail page: Metrics (default, includes the cost/traffic/uptime overview blocks), Network, Traffic, Security, IP Quality. Tab and time range are URL-driven (`?tab=`, `?range=`); the `range` window is shared across tabs.
- **Standalone public network page** — `/status/network/$serverId`; exists only as a fallback for status pages configured with `show_server_detail=false` but `show_network=true` (see ADR-0001).
- **Live metrics (实时指标帧)** — the partial projection of a server carried by WS `update` frames: the fields an agent report can populate (usage, speeds, loads, connection counts), plus `name` kept purely for decoder compatibility. Static facts are absent from the wire; clients keep their cached values when merging (see ADR-0002). _Avoid_: partial status, update payload.
- **Full server status** — the complete ~35-field snapshot (`ServerStatus`) carried by `full_sync` and REST. The only sources allowed to seed the catalog or overwrite static facts (totals, os, tags, geo, enrollment).
