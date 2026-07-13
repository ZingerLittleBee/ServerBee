# Agent Authority 生命周期冒烟测试

验证 Server onboarding、offer 单次消费、Agent 自持 run token、重新接入、精确 offer CAS、authority 吊销和事件留存。环境与启动参考 [README.md](README.md)。

## 0. 登录并准备变量

```bash
BASE=http://localhost:9527
COOKIE=/tmp/sb-agent-authority.txt
curl -fsS -c "$COOKIE" -X POST "$BASE/api/auth/login" \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin123"}'
REQUEST_ID=$(uuidgen)
```

## 1. 幂等 onboarding

```bash
BODY="{\"onboarding_request_id\":\"$REQUEST_ID\",\"name\":\"Lifecycle Smoke\",\"ttl_secs\":600}"
CREATED=$(curl -fsS -b "$COOKIE" -X POST "$BASE/api/servers" \
  -H 'Content-Type: application/json' -d "$BODY")
SERVER_ID=$(printf '%s' "$CREATED" | jq -r '.data.server_id')
OFFER_ID=$(printf '%s' "$CREATED" | jq -r '.data.enrollment.id')
CODE=$(printf '%s' "$CREATED" | jq -r '.data.enrollment.code')
curl -fsS -b "$COOKIE" -X POST "$BASE/api/servers" \
  -H 'Content-Type: application/json' -d "$BODY" | jq .
```

预期：首次响应 `replayed=false` 并返回一次明文 code；重试响应 `replayed=true`、`enrollment=null`，且 `outstanding_offer.id` 等于 `$OFFER_ID`。相同 request ID 搭配不同输入返回 `409 ONBOARDING_IDEMPOTENCY_CONFLICT`。

## 2. Agent 提议并持有 run token

```bash
RUN_TOKEN=$(openssl rand -base64 32 | tr -d '\n')
curl -fsS -X POST "$BASE/api/agent/register" \
  -H "Authorization: Bearer $CODE" \
  -H 'Content-Type: application/json' \
  -d "{\"proposed_run_token\":\"$RUN_TOKEN\"}" | jq .
```

预期：只返回 `$SERVER_ID`，不返回 token。再次使用 `$CODE` claim 返回 401。`GET /api/servers/$SERVER_ID/agent-authority` 返回 `status=claimed` 且没有 outstanding offer；`websocat "ws://localhost:9527/api/agent/ws?token=$RUN_TOKEN"` 可以握手。

## 3. Graceful 重新接入与精确替换

```bash
GRACEFUL=$(curl -fsS -b "$COOKIE" -X POST \
  "$BASE/api/servers/$SERVER_ID/agent-authority/re-enrollment" \
  -H 'Content-Type: application/json' -d '{"mode":"graceful"}')
GRACEFUL_ID=$(printf '%s' "$GRACEFUL" | jq -r '.data.enrollment.id')

REPLACED=$(curl -fsS -b "$COOKIE" -X POST \
  "$BASE/api/servers/$SERVER_ID/agent-authority/offers/$GRACEFUL_ID/replace")
NEW_OFFER_ID=$(printf '%s' "$REPLACED" | jq -r '.data.enrollment.id')
```

预期：graceful 后 authority 仍为 `claimed`，旧 run token 仍可连接。替换后 `$GRACEFUL_ID` 进入 `replaced` 终态并返回一次新 code；再次替换旧 ID 返回 409，且不能覆盖 `$NEW_OFFER_ID`。

先精确吊销当前 offer，为 emergency 场景清空 outstanding 状态：

```bash
curl -fsS -b "$COOKIE" -X DELETE \
  "$BASE/api/servers/$SERVER_ID/agent-authority/offers/$NEW_OFFER_ID"
```

## 4. Emergency 重新接入与连接 fencing

```bash
EMERGENCY=$(curl -fsS -b "$COOKIE" -X POST \
  "$BASE/api/servers/$SERVER_ID/agent-authority/re-enrollment" \
  -H 'Content-Type: application/json' -d '{"mode":"emergency"}')
EMERGENCY_CODE=$(printf '%s' "$EMERGENCY" | jq -r '.data.enrollment.code')
NEW_RUN_TOKEN=$(openssl rand -base64 32 | tr -d '\n')
```

预期：authority 立即变为 `unclaimed`，现有旧 WebSocket 被关闭，旧 `$RUN_TOKEN` 新握手返回 401。随后用 `$EMERGENCY_CODE` 和 `$NEW_RUN_TOKEN` claim，状态回到 `claimed`，新 token 可连接。

## 5. 独立吊销 authority

```bash
curl -fsS -b "$COOKIE" -X DELETE \
  "$BASE/api/servers/$SERVER_ID/agent-authority" | jq .
curl -fsS -b "$COOKIE" "$BASE/api/servers/$SERVER_ID/agent-authority" | jq .
```

预期：返回 `changed=true`，状态为 `unclaimed`，新 WebSocket 被隔离且没有自动生成 offer。此时可通过 `POST /api/servers/$SERVER_ID/agent-authority/offers` 发出 offer；再用准确 offer ID 删除它。

## 6. 事件与删除留存

```bash
curl -fsS -b "$COOKIE" \
  "$BASE/api/agent-authority/events?server_id=$SERVER_ID&limit=100" | jq .
curl -fsS -b "$COOKIE" -X DELETE "$BASE/api/servers/$SERVER_ID"
curl -fsS -b "$COOKIE" \
  "$BASE/api/agent-authority/events?server_id=$SERVER_ID&limit=100" | jq .
```

预期：历史包含 offer issued/replaced/revoked/consumed、graceful/emergency、authority revoked 和 server deleted 等转换，不含任何明文 code 或 run token；删除 Server 后事件仍可读取。

## 7. UI 冒烟

- **Add Server** 在失败后使用相同 onboarding request ID 重试，显式关闭后才生成新 ID。
- 重放响应不尝试恢复明文，只允许精确替换响应中可见的 outstanding offer。
- Server 详情分别提供 Graceful、Emergency、精确替换/吊销 offer，以及独立的 Agent Authority 吊销确认。
- Web 与 iOS 都从 `agent_authority` 判断 claimed/unclaimed 和 outstanding 状态。

## 自动化回归对照

```bash
cargo test -p serverbee-server --test agent_registration_integration
cargo test -p serverbee-server service::agent_authority
cargo test -p serverbee-agent register
```
