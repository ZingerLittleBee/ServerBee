# Agent Upgrade QA Checklist

Tests the agent self-upgrade feature from the server detail page.

**Prerequisites:**
- Server and agent running (see [README.md](README.md) for setup)
- Agent has `CAP_UPGRADE` capability enabled (default for new registrations)
- Admin user logged in

---

## Test Cases

### 1. Trigger Upgrade from Server Detail Page

**Steps:**
1. Navigate to `/servers/:id` (server detail page)
2. Click the "Upgrade Agent" button in the server header/actions area
3. Confirm the upgrade dialog

**Expected:**
- Upgrade dialog shows current version and confirms upgrade action
- After confirmation, upgrade progress panel appears
- WebSocket connection remains active during upgrade

---

### 2. Verify Progress Stages

**Steps:**
1. Trigger an upgrade (see Test 1)
2. Observe the progress indicator during upgrade

**Expected:**
- Progress stages appear in sequence:
  - `downloading` -- Downloading new binary from release URL
  - `verifying` -- Verifying SHA-256 checksum
  - `preflight` -- Running preflight checks
  - `installing` -- Installing new binary
  - `restarting` -- Restarting agent process
- Each stage shows appropriate status icon and message
- Progress bar advances through stages

---

### 3. Verify Success State and Version Update

**Steps:**
1. Wait for upgrade to complete (typically 10-30 seconds)
2. Observe final status

**Expected:**
- Success message displayed: "Agent upgraded successfully"
- New version number shown matches target version
- Agent reconnects automatically after restart
- Server detail page shows updated version in header
- No manual refresh required -- updates via WebSocket

---

### 4. Verify Failed State with Error Message

**Steps:**
1. Configure an invalid `release_base_url` in server config (temporarily)
2. Trigger upgrade
3. Wait for failure

**Expected:**
- Error state displayed with specific error message
- Backup path shown if backup was created before failure
- Retry button available to attempt upgrade again
- Agent remains in working state (rollback successful)

---

### 5. Verify Timeout Handling

**Steps:**
1. Trigger upgrade
2. Simulate network issues or use very slow connection
3. Wait for timeout (default 5 minutes)

**Expected:**
- Timeout error displayed after configured timeout period
- Upgrade marked as failed
- Agent continues running existing version
- No partial installation corruption

---

### 6. Verify Concurrent Upgrade Rejection

**Steps:**
1. Start an upgrade on server A
2. While upgrade is in progress, attempt to start upgrade on server B
3. Or rapidly click upgrade button multiple times on same server

**Expected:**
- Second upgrade attempt rejected with "Upgrade already in progress" message
- UI prevents concurrent upgrade initiation
- First upgrade continues unaffected

---

### 7. Verify Admin-Only Access Control

**Steps:**
1. Log in as Member (non-admin) user
2. Navigate to server detail page
3. Attempt to trigger upgrade

**Expected:**
- "Upgrade Agent" button is hidden or disabled
- Direct API call returns 403 Forbidden:
  ```bash
  curl -X POST http://localhost:9527/api/servers/:id/upgrade \
    -H "Authorization: Bearer $MEMBER_TOKEN" \
    -d '{"version":"latest"}'
  # Expected: {"error":"Admin access required"}
  ```

---

### 8. Test WebSocket Real-Time Updates

**Steps:**
1. Open browser DevTools Network tab
2. Connect to WebSocket `/api/ws/servers`
3. Trigger upgrade from another browser/session
4. Monitor WebSocket messages

**Expected:**
- `CapabilitiesChanged` message received when upgrade starts (capability temporarily disabled)
- `ServerUpdate` messages with upgrade progress in payload
- `CapabilitiesChanged` message received when upgrade completes (capability re-enabled)
- UI updates in real-time without page refresh

---

### 9. Verify Capability Check

**Steps:**
1. Disable `CAP_UPGRADE` on a server (via database or API)
2. Navigate to that server's detail page

**Expected:**
- "Upgrade Agent" button is hidden
- Upgrade option not available in UI
- Attempting upgrade via API returns capability error

---

### 10. Rollback Verification

**Steps:**
1. Trigger upgrade
2. During `installing` stage, force agent disconnect (kill process)
3. Restart agent manually

**Expected:**
- Agent starts with previous version (backup restored)
- Server detects version mismatch on reconnect
- Upgrade can be retried

---

## API Endpoints

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/api/servers/:id/upgrade` | POST | Admin | Trigger agent upgrade |
| `/api/servers/:id/upgrade-status` | GET | Admin | Get current upgrade status |
| `/api/ws/servers` | WS | Session | Real-time upgrade progress |

---

## Related Files

- `crates/server/src/service/upgrade.rs` -- Server upgrade service
- `crates/agent/src/upgrade.rs` -- Agent upgrade handler
- `apps/web/src/components/server/upgrade-panel.tsx` -- UI component
