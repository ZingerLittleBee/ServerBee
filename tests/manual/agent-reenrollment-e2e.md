# Agent Re-enrollment End-to-End VPS 验证

在真实 Linux VPS 上验证 Agent Authority 的 graceful/emergency 重新接入、Agent 侧 token 暂存、精确 offer CAS 和 WebSocket fencing。构建与部署当前分支的方法见 [full-deploy-e2e.md](full-deploy-e2e.md)。

## 0. 前提

- 当前分支的 Server 和 Agent 已部署到专用测试 VPS。
- 已有一个 `claimed` 且在线的 Server，记录其 `$SERVER_ID`。
- 管理员 Cookie 保存到 `/tmp/sb.cookies`，Server 地址为 `$BASE`。
- VPS 上 Agent 二进制位于 `/opt/serverbee/bin/serverbee-agent`，正式配置位于 `/opt/serverbee/etc/agent.toml`。
- 测试凭据只放环境变量或 vault，不入仓库。

```bash
export BASE=https://<your-test-host>
export SERVER_ID=<server-id>
export VPS_HOST=root@<your-vps>
```

先确认基线：

```bash
curl -fsS -b /tmp/sb.cookies \
  "$BASE/api/servers/$SERVER_ID/agent-authority" | jq .
```

预期 `status=claimed`、`outstanding_offer=null`，Agent 日志持续上报。

## 1. Graceful re-enrollment

```bash
GRACEFUL=$(curl -fsS -b /tmp/sb.cookies -X POST \
  "$BASE/api/servers/$SERVER_ID/agent-authority/re-enrollment" \
  -H 'Content-Type: application/json' \
  -d '{"mode":"graceful","ttl_secs":600}')
GRACEFUL_ID=$(printf '%s' "$GRACEFUL" | jq -r '.data.enrollment.id')
GRACEFUL_CODE=$(printf '%s' "$GRACEFUL" | jq -r '.data.enrollment.code')
```

检查：

- Authority 仍为 `claimed`，outstanding offer ID 为 `$GRACEFUL_ID`。
- 原 Agent 连接和指标上报不中断。
- 明文 code 仅在该响应和当前 UI 结果视图出现一次。

### 1.1 精确替换的 CAS

```bash
REPLACED=$(curl -fsS -b /tmp/sb.cookies -X POST \
  "$BASE/api/servers/$SERVER_ID/agent-authority/offers/$GRACEFUL_ID/replace")
REPLACEMENT_ID=$(printf '%s' "$REPLACED" | jq -r '.data.enrollment.id')
REPLACEMENT_CODE=$(printf '%s' "$REPLACED" | jq -r '.data.enrollment.code')

curl -sS -o /tmp/stale.json -w '%{http_code}\n' -b /tmp/sb.cookies -X POST \
  "$BASE/api/servers/$SERVER_ID/agent-authority/offers/$GRACEFUL_ID/replace"
```

预期第二次替换旧 ID 返回 409，当前 outstanding offer 仍是 `$REPLACEMENT_ID`。

### 1.2 用真实 Agent 完成 claim

在 VPS 上建立临时工作目录，让同一 Agent 二进制使用独立 `agent.toml`。不要复制旧 token：

```bash
ssh "$VPS_HOST" "rm -rf /tmp/serverbee-reenroll && mkdir -p /tmp/serverbee-reenroll && cat > /tmp/serverbee-reenroll/agent.toml <<EOF
server_url = \"$BASE\"
enrollment_code = \"$REPLACEMENT_CODE\"
EOF
cd /tmp/serverbee-reenroll && /opt/serverbee/bin/serverbee-agent > agent.log 2>&1 & echo \$! > agent.pid"
```

预期临时 Agent 在 claim 前先把随机 run token 写入临时 `agent.toml`，Server 只返回 `server_id`。新 claim 成功后原连接被 fenced，临时 Agent 保持在线。检查日志不能出现 enrollment code 或 run token 明文。

停止临时进程，把它持久化后的 `token` 安全写回正式配置，再重启正式服务。确认不再需要 enrollment code 即可重新连接。

## 2. Emergency re-enrollment

恢复到 `claimed` 且无 outstanding offer 后执行：

```bash
EMERGENCY=$(curl -fsS -b /tmp/sb.cookies -X POST \
  "$BASE/api/servers/$SERVER_ID/agent-authority/re-enrollment" \
  -H 'Content-Type: application/json' \
  -d '{"mode":"emergency","ttl_secs":600}')
EMERGENCY_ID=$(printf '%s' "$EMERGENCY" | jq -r '.data.enrollment.id')
EMERGENCY_CODE=$(printf '%s' "$EMERGENCY" | jq -r '.data.enrollment.code')
```

预期同一状态转换内完成：

- authority 立即变为 `unclaimed`；
- 旧 WebSocket 被关闭，旧 token 新握手返回 401；
- `$EMERGENCY_ID` 成为唯一 outstanding offer；
- 使用临时 Agent 配置和 `$EMERGENCY_CODE` claim 后恢复 `claimed`。

## 3. Offer revoke 与 authority revoke 是两件事

发出一个 graceful offer 后，先精确吊销 offer：

```bash
curl -fsS -b /tmp/sb.cookies -X DELETE \
  "$BASE/api/servers/$SERVER_ID/agent-authority/offers/<exact-offer-id>"
```

预期 authority 仍为 `claimed`。随后独立吊销 authority：

```bash
curl -fsS -b /tmp/sb.cookies -X DELETE \
  "$BASE/api/servers/$SERVER_ID/agent-authority"
```

预期 authority 变为 `unclaimed`、连接被 fenced，并且不会隐式生成 offer。需要恢复时显式调用：

```bash
curl -fsS -b /tmp/sb.cookies -X POST \
  "$BASE/api/servers/$SERVER_ID/agent-authority/offers" \
  -H 'Content-Type: application/json' -d '{"ttl_secs":600}'
```

## 4. UI 与事件历史

- Web 和 iOS 都提供 Graceful/Emergency 两种明确模式。
- 有 outstanding offer 时，只能精确替换或吊销当前可见 ID。
- Agent Authority 吊销有独立的破坏性确认，不假装会生成新 code。
- 刷新或重放后只显示 offer 元数据，不恢复明文 code。

```bash
curl -fsS -b /tmp/sb.cookies \
  "$BASE/api/agent-authority/events?server_id=$SERVER_ID&limit=100" | jq .
```

预期历史包含 actor/source、graceful/emergency mode、offer terminal outcome、authority before/after；任何事件都不含明文密钥。

## 5. 清场

```bash
ssh "$VPS_HOST" 'if [ -f /tmp/serverbee-reenroll/agent.pid ]; then kill "$(cat /tmp/serverbee-reenroll/agent.pid)" 2>/dev/null || true; fi; rm -rf /tmp/serverbee-reenroll'
```

确保正式 Agent 使用最新持久化 token 且已在线。若最终保留 `unclaimed` 状态，明确记录并删除 outstanding offer，别给下一位测试者留一枚薛定谔的注册码。
