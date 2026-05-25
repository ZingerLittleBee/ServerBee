# IP Quality (ipapi.is) Manual Verification

This file lives alongside the test plan for IP Quality. Run these checks after any change to `crates/server/src/service/ip_risk.rs` or `IpQualityConfig`.

## Prerequisites
- A fresh SQLite database (or one with the 2026-05-25 migration applied).
- An agent able to reach the test server.

## Checklist

1. **Default zero-config works.**
   Start the server with no `SERVERBEE_IP_QUALITY__*` env vars set. Connect an agent. Open the IP Quality page in the UI.
   - Expect: real `risk_level` populated (not `unknown`); badges for `proxy`/`vpn`/`hosting` reflect the IP's true category.

2. **Fallback triggers when primary is unreachable.**
   Set `SERVERBEE_IP_QUALITY__IPAPI_IS__ENDPOINT=https://example.invalid` and restart.
   - Expect: server log shows `primary failed for <ip>, attempting fallback` then `provider ip-api succeeded`. UI shows geo + proxy flags but `risk_score` is blank.

3. **`risk_provider=none` disables scoring.**
   Set `SERVERBEE_IP_QUALITY__RISK_PROVIDER=none`.
   - Expect: UI shows `risk_level = unknown`, geo still populated from MMDB.

4. **API key flows correctly.**
   Set `SERVERBEE_IP_QUALITY__IPAPI_IS__API_KEY=test123`. Capture outbound HTTPS request (e.g. `tcpdump -s0 -w /tmp/cap.pcap host api.ipapi.is`).
   - Expect: query string contains `&key=test123`.

5. **Legacy env vars are ignored, not erroring.**
   Set `SERVERBEE_IP_QUALITY__SCAMALYTICS__API_KEY=xxx`. Start server.
   - Expect: clean startup, no error. `risk_provider` resolves to default `ipapi_is`.

6. **New columns persist.**
   After an agent reports its egress IP, query the DB:
   ```sql
   SELECT ip, risk_score, is_tor, is_abuser, is_mobile, asn_abuser_score, abuse_email
   FROM ip_quality_snapshot LIMIT 5;
   ```
   - Expect: columns exist; populated values look reasonable for the IP type.
