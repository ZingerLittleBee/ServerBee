# 05 API 密钥 — 冒烟测试

**前置条件**:已登录 admin,进入 `/settings/api-keys`。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| K1 | 创建 API Key | 新建 key(命名)→ 创建 | 返回 `serverbee_` 前缀明文 key(仅此一次显示) | 是 | ✅ |
| K2 | 使用 API Key | `curl -H "X-API-Key: <key>" /api/servers` | 返回 200 + 数据 | 是 | ✅ |
| K3 | 无效 Key | 用错误 key 请求 | 401 未授权 | 是 | ✅ |
| K4 | 删除 Key | 删除已创建 key | 列表移除,旧 key 请求即失效 401 | 是 | ✅ |
| K5 | Key 列表 | 查看列表 | 显示名称/创建时间,不回显明文 | 否 | ✅ |

> ✅ 路径 `POST/GET/DELETE /api/auth/api-keys`。K1 返回 `serverbee_` 前缀 53 位明文(仅一次);K2 持 key 调 `/api/servers`→200;K3 无效 key→401;K4 删除后旧 key→401;K5 列表仅含 `key_prefix`,无明文泄露。

**汇总**:✅ 5 / ❌ 0 / — 0
