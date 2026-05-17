# Agent Enrollment Code 冒烟测试

验证「一次性 enrollment code 取代共享 auto_discovery_key」改造的关键路径与安全属性。
环境与启动参考 [README.md](README.md) 的「启动本地环境」。Server 默认 `http://localhost:9527`，管理员用户名 `admin`。

通过标准：步骤 1–7、9 全部符合预期，步骤 10 端到端闭环成功，步骤 11 UI 正常。步骤 8 为可选耗时项。

---

## 0. 启动环境

```bash
cd <repo-or-worktree-root>
SERVERBEE_ADMIN__PASSWORD=admin123 SERVERBEE_AUTH__SECURE_COOKIE=false cargo run -p serverbee-server
```

预期：启动 banner **不再打印** `Auto-discovery key`（旧机制已移除）。未设密码时从 banner 的
`*** IMPORTANT: Save these now ***` 区块读取 `Admin password`。

## 1. 管理员登录

```bash
curl -s -c /tmp/sb.txt -X POST http://localhost:9527/api/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin123"}'
```

预期：HTTP 200。

## 2. 铸造 enrollment code（golden path）

```bash
curl -s -b /tmp/sb.txt -X POST http://localhost:9527/api/agent/enrollments \
  -H 'Content-Type: application/json' -d '{}'
```

预期：`{"data":{"id":"...","code":"<43 位>","expires_at":"..."}}`。记录 `CODE` 与 `ID`。

## 3. 注册 agent（消费 code）

```bash
curl -s -X POST http://localhost:9527/api/agent/register \
  -H "Authorization: Bearer $CODE" \
  -H 'Content-Type: application/json' -d '{"fingerprint":""}'
```

预期：HTTP 200，返回 `server_id` + `token`。记录二者（`SERVER_ID` / `OLD_TOKEN`）。

## 4. 单次性校验（核心安全属性）

```bash
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://localhost:9527/api/agent/register \
  -H "Authorization: Bearer $CODE"
```

预期：**401**——同一 code 已消费，不可重放。

## 5. 列表不泄露明文

```bash
curl -s -b /tmp/sb.txt http://localhost:9527/api/agent/enrollments
```

预期：数组包含该条，仅含 `code_prefix`（8 位）与非空 `consumed_at`；**无** `code` / `code_hash` 字段。

## 6. 旧机制确已移除

```bash
# 旧共享 key 注册方式
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://localhost:9527/api/agent/register \
  -H 'Authorization: Bearer test-key'                          # 预期 401
# 旧设置端点
curl -s -o /dev/null -w '%{http_code}\n' -b /tmp/sb.txt \
  http://localhost:9527/api/settings/auto-discovery-key        # 预期 404
```

## 7. Token 轮换 + 吊销旧 token

```bash
curl -s -b /tmp/sb.txt -X POST http://localhost:9527/api/agent/$SERVER_ID/rotate-token
```

预期：HTTP 200，返回新 `token` ≠ `OLD_TOKEN`。再用旧 token 连 WS：

```bash
curl -s -o /dev/null -w '%{http_code}\n' \
  "http://localhost:9527/api/agent/ws?token=$OLD_TOKEN"        # 预期 401
```

## 8. TTL 过期（可选，耗时）

```bash
SHORT=$(curl -s -b /tmp/sb.txt -X POST http://localhost:9527/api/agent/enrollments \
  -H 'Content-Type: application/json' -d '{"ttl_secs":2}' | grep -o '"code":"[^"]*"' | cut -d'"' -f4)
sleep 3
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://localhost:9527/api/agent/register \
  -H "Authorization: Bearer $SHORT"                            # 预期 401
```

## 9. 删除

```bash
curl -s -o /dev/null -w '%{http_code}\n' -b /tmp/sb.txt \
  -X DELETE http://localhost:9527/api/agent/enrollments/$ID    # 预期 200
```

## 10. 端到端真实 agent（完整闭环）

1. 按步骤 2 铸造新 code。
2. 写 `agent.toml`：

   ```toml
   server_url = "http://localhost:9527"
   enrollment_code = "<新 CODE>"
   ```

   或 `SERVERBEE_SERVER_URL=http://127.0.0.1:9527 SERVERBEE_ENROLLMENT_CODE=<CODE> cargo run -p serverbee-agent`。
3. 预期：agent 日志出现 `Registered as server_id=...` → `Registration successful`，token 落盘到 `agent.toml`。
4. 重启 agent：使用已存 token 直连，**不再消费 code**（验证 code 仅首次需要）。
5. 故意用过期/错误 code 启动：agent 应打印
   `Registration failed: HTTP 401 ... enrollment code ... expired or already used`（验证错误透传）。

## 11. UI 冒烟（Settings 页，对应 [registration-hardening.md](registration-hardening.md) RH-5）

通过 `make web-dev`（或 build 后）访问 `/settings`：

- 点击「生成 enrollment code」→ 一次性显示 code 与可复制安装命令（含 `--enrollment-code` 与当前 origin）。
- 列表显示该条（prefix + 状态徽章：active / consumed / expired），删除按钮带确认对话框。
- 刷新页面后明文 code 不再出现（仅展示一次）。

---

## 自动化回归对照

以下属性已有自动化测试覆盖（`cargo test -p serverbee-server`），冒烟仅作端到端复核：

| 属性 | 测试 |
|------|------|
| 单次消费 + 并发抢兑竞态 | `service::enrollment` 单元测试 |
| TTL 过期 / prune | `service::enrollment` 单元测试 |
| 列表 DTO 不含 code/hash | `enrollment_summary_dto_never_exposes_code_or_hash` |
| 注册消费 + 重放拒绝 | `register_flow_consumes_code_single_use` |
| 轮换后旧 token 被 401 拒绝 | `integration.rs` e2e 测试 |
