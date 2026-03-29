# 自动注册加固测试用例

## 前置条件

参照 [README.md](README.md) 中的「启动本地环境」部分完成 Server 启动和管理员登录。

如果要验证 UI 中的 cleanup 行为，需要额外准备两个占位服务器：

1. 创建一个“离线占位 server”：只调用注册接口，不建立 WebSocket 连接。
2. 创建一个“在线但未初始化的占位 server”：调用注册接口后，仅建立 `/api/agent/ws?token=...` 连接并停在 Welcome，不发送 `SystemInfo`。

可直接复用下面的命令：

```bash
# 1. 管理员登录
curl -s -c /tmp/sb-cookies.txt -X POST http://localhost:9527/api/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin123"}'

# 2. 创建离线占位 server
curl -s -X POST http://localhost:9527/api/agent/register \
  -H 'Authorization: Bearer test-key'

# 3. 创建带固定 fingerprint 的 server（可重复调用验证复用）
curl -s -X POST http://localhost:9527/api/agent/register \
  -H 'Authorization: Bearer test-key' \
  -H 'Content-Type: application/json' \
  -d '{"fingerprint":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}'
```

---

## 一、自动化测试覆盖

### 单元测试

| 测试组 | 文件 | 验证内容 |
|--------|------|----------|
| Agent 指纹 | `crates/agent/src/fingerprint.rs` | 指纹只基于 `machine-id` 生成；同一 `machine-id` 哈希稳定 |
| Servers 列表 cleanup 计数 | `apps/web/src/lib/orphan-server-utils.test.ts` | 仅将离线且未初始化的 `New Server` 计入 cleanup 候选 |
| Cleanup 辅助逻辑 | `crates/server/src/router/api/server.rs` | `collect_orphan_server_ids` 会跳过在线占位 server |

### 集成测试

| 测试组 | 文件 | 验证内容 |
|--------|------|----------|
| 自动注册复用 | `crates/server/tests/integration.rs` | 相同 fingerprint 重复注册时复用同一 `server_id`，并轮换 token |
| Cleanup 在线保护 | `crates/server/tests/integration.rs` | `DELETE /api/servers/cleanup` 只删除离线 orphan，不删除已在线但尚未上报 `SystemInfo` 的 server |

---

## 二、API 与后端行为

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| RH-1 | 相同 fingerprint 复用同一 server | 连续两次调用 `POST /api/agent/register`，请求体都带相同 64 位 hex fingerprint | 两次返回相同 `server_id`，第二次返回新 token，`GET /api/servers` 仅有 1 条记录 | ✅ |
| RH-2 | Cleanup 仅删除离线 orphan | 创建 1 个离线占位 server 和 1 个在线未初始化 server，然后调用 `DELETE /api/servers/cleanup` | 返回 `deleted_count=1`；离线 orphan 被删除，在线未初始化 server 保留 | ✅ |

---

## 三、Servers 列表页（/servers）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| RH-3 | Cleanup 按钮计数只包含离线 orphan | 打开 `/servers`，保证存在 1 个离线 orphan 和 1 个在线未初始化 server | 列表页显示 `Clean up unconnected (1)`，不会把在线未初始化 server 算进去 | ✅ agent-browser 实测 |
| RH-4 | Cleanup 操作不误删在线未初始化 server | 点击 cleanup 按钮并确认删除 | cleanup 后 `GET /api/servers` 仅剩在线占位 server，cleanup 按钮消失 | ✅ agent-browser + API 复核 |

---

## 四、Settings 页（/settings）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| RH-5 | UI 重新生成 discovery key | 打开 `/settings`，显示 key 后点击 Regenerate 并确认 | 新 key 与旧 key 不同；本轮验证从 `test-key` 变为 `Su6GKY9teQFy9psueUb5j371uNWpo8xefFTV_EZ3VJY` | ✅ agent-browser 实测 |

---

## 五、Docker 安装输出

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| RH-6 | Docker agent compose 包含 machine-id 挂载 | 运行 `serverbee install agent --method docker` 或直接检查生成的 compose 模板 | `docker-compose.agent.yml` 中包含 `- /etc/machine-id:/etc/machine-id:ro` | ✅ 模板检查通过 |

---

## 测试统计

| 模块 | 用例数 | ✅ | ⏭️ | — |
|------|--------|-----|------|-----|
| 自动化测试覆盖 | 5 | 5 | 0 | 0 |
| API 与后端行为 | 2 | 2 | 0 | 0 |
| Servers 列表页 | 2 | 2 | 0 | 0 |
| Settings 页 | 1 | 1 | 0 | 0 |
| Docker 安装输出 | 1 | 1 | 0 | 0 |
| **合计** | **11** | **11** | **0** | **0** |
