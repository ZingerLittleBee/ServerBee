# Alert & Notification E2E Verification Design

## Goal

End-to-end verification of the complete alert→notification pipeline using all 4 notification channels (Webhook, Telegram, Bark, Email) with real credentials, via browser UI operations.

## Scope

1. Fix UI form bugs (Notifications Add button, API Keys Create button)
2. Create all 4 notification channels via UI
3. Verify test notification sends to all channels
4. Verify threshold-based alert triggers notification (CPU > 1%)
5. Verify offline-based alert triggers notification (Agent stopped)
6. Verify alert resolves when condition clears

## Credential Configuration

File: `.env.test` (project root, gitignored)

```bash
# Webhook
TEST_WEBHOOK_URL=https://httpbin.org/post

# Telegram
TEST_TELEGRAM_BOT_TOKEN=123456:ABC-DEF...
TEST_TELEGRAM_CHAT_ID=-1001234567890

# Bark
TEST_BARK_SERVER_URL=https://api.day.app
TEST_BARK_DEVICE_KEY=your_device_key

# Email (SMTP)
TEST_EMAIL_SMTP_HOST=smtp.gmail.com
TEST_EMAIL_SMTP_PORT=587
TEST_EMAIL_USERNAME=your@gmail.com
TEST_EMAIL_PASSWORD=app_password
TEST_EMAIL_FROM=your@gmail.com
TEST_EMAIL_TO=target@example.com
```

Loaded before E2E execution. Not committed to git.

## UI Bug Fix

Two issues found during prior E2E testing:

1. **Notifications Add button** — Clicking "+ Add" did not visibly open the form. Source code shows correct `showForm` state toggle. Need to diagnose in browser: timing issue vs rendering bug.

2. **API Keys Create button** — Clicking "+ Create" did not create a key. Source code shows correct mutation to `/api/auth/api-keys`. Need to diagnose: API error swallowed silently vs query invalidation issue.

Fix approach: Run Vite dev server, use agent-browser to step through each interaction, screenshot between steps to isolate the failure point.

## Verification Flow

### Phase 1: Environment Setup

- Load `.env.test` credentials
- Confirm Server (`localhost:9527`) + Agent running
- Login via browser as admin

### Phase 2: Create Notification Channels (Browser UI)

On `/settings/notifications`, create 4 channels:

| Channel | Fields |
|---------|--------|
| Webhook | name, url |
| Telegram | name, bot_token, chat_id |
| Bark | name, server_url, device_key |
| Email | name, smtp_host, smtp_port, username, password, from, to |

After each: screenshot to verify list update.

Then create a Notification Group linking all 4 channels.

### Phase 3: Test Notification

For each channel, click the test send button (paper plane icon).

Verify receipt:
- Webhook: check httpbin response or local endpoint
- Telegram: message appears in chat
- Bark: push notification on device
- Email: message in inbox

### Phase 4: Threshold Alert (CPU > 1%)

1. Navigate to `/settings/alerts`
2. Create rule:
   - Name: "High CPU Test"
   - Rule: `cpu > 1%` (min: 1.0)
   - trigger_mode: "always"
   - notification_group: the group from Phase 2
   - cover_type: "all"
3. Wait 60-120s for evaluation cycle
4. Verify all 4 channels receive alert notification
5. Screenshot alert state in UI

### Phase 5: Offline Alert

1. Create rule:
   - Name: "Server Offline Test"
   - Rule: `offline`, duration: 60s
   - trigger_mode: "once"
   - notification_group: same group
   - cover_type: "all"
2. Stop Agent process (`kill` the Agent PID)
3. Wait ~90s (30s offline detection + 60s evaluation)
4. Verify all 4 channels receive offline alert
5. Restart Agent
6. Wait ~120s for recovery
7. Verify alert state resolves in UI

### Phase 6: Cleanup

- Delete test alert rules
- Delete notification group
- Delete notification channels
- Update TESTING.md with verification results

## Output

- Screenshots at each phase saved to `/tmp/sb-e2e-alert-*.png`
- TESTING.md updated with alert/notification verification checklist
- PROGRESS.md updated

## Dependencies

- Server + Agent running on localhost:9527
- `.env.test` with valid credentials
- agent-browser installed (via npx)
- Vite dev server for UI bug diagnosis (temporary)
