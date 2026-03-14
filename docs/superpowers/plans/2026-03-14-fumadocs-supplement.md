# Fumadocs 文档补充实现计划

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 全面审查并修复现有 11 个文档的过时内容，新增 5 个页面补充 P2~P5 缺失功能文档，中英文双语同步。

**Architecture:** 保留现有文档结构不变（快速开始/指南/功能/参考），在"功能"和"参考"之间新增"管理"分组。修复环境变量映射错误（中文文档 `_` → `__`），更新标记为"计划中"但已实现的功能描述。

**Tech Stack:** Fumadocs MDX (content/docs/cn/ + content/docs/en/)，meta.json 导航配置

---

## Chunk 1: 修复现有中文文档过时内容

### Task 1: 修复 configuration.mdx 环境变量映射 (CN)

**Files:**
- Modify: `apps/fumadocs/content/docs/cn/configuration.mdx`

代码中使用 `Env::prefixed("SB_").split("__")` (双下划线)，中文文档错误使用单下划线。

- [ ] **Step 1: 修复环境变量说明文字**

将 "使用 `_` 作为层级分隔符" 相关描述改为 "使用 `__`（双下划线）作为嵌套键分隔符"。

- [ ] **Step 2: 修复环境变量映射表**

替换所有单下划线映射为双下划线：

```
修复前                              修复后
SB_SERVER_LISTEN               →   SB_SERVER__LISTEN
SB_SERVER_DATA_DIR             →   SB_SERVER__DATA_DIR
SB_DATABASE_PATH               →   SB_DATABASE__PATH
SB_DATABASE_MAX_CONNECTIONS    →   SB_DATABASE__MAX_CONNECTIONS
SB_AUTH_SESSION_TTL            →   SB_AUTH__SESSION_TTL
SB_AUTH_AUTO_DISCOVERY_KEY     →   SB_AUTH__AUTO_DISCOVERY_KEY
SB_AUTH_SECURE_COOKIE          →   SB_AUTH__SECURE_COOKIE
SB_ADMIN_USERNAME              →   SB_ADMIN__USERNAME
SB_ADMIN_PASSWORD              →   SB_ADMIN__PASSWORD
SB_RETENTION_RECORDS_DAYS      →   SB_RETENTION__RECORDS_DAYS
SB_LOG_LEVEL                   →   SB_LOG__LEVEL
SB_GEOIP_ENABLED               →   SB_GEOIP__ENABLED
SB_GEOIP_MMDB_PATH             →   SB_GEOIP__MMDB_PATH
SB_OAUTH_BASE_URL              →   SB_OAUTH__BASE_URL
SB_OAUTH_GITHUB_CLIENT_ID     →   SB_OAUTH__GITHUB__CLIENT_ID
SB_COLLECTOR_INTERVAL          →   SB_COLLECTOR__INTERVAL
SB_COLLECTOR_ENABLE_GPU        →   SB_COLLECTOR__ENABLE_GPU
```

- [ ] **Step 3: 修复内联环境变量示例**

文件中任何内联代码示例中的 `SB_xxx_yyy` 改为 `SB_xxx__yyy`。

---

### Task 2: 修复 quick-start.mdx 环境变量 (CN)

**Files:**
- Modify: `apps/fumadocs/content/docs/cn/quick-start.mdx`

- [ ] **Step 1: 修复 Docker Compose 示例中的环境变量**

```yaml
# 修复前
environment:
  - SB_ADMIN_PASSWORD=changeme

# 修复后
environment:
  - SB_ADMIN__PASSWORD=changeme
```

- [ ] **Step 2: 修复其他环境变量引用**

文件中所有 `SB_AUTH_AUTO_DISCOVERY_KEY` → `SB_AUTH__AUTO_DISCOVERY_KEY`，`SB_ADMIN_PASSWORD` → `SB_ADMIN__PASSWORD` 等。

---

### Task 3: 修复 server.mdx 环境变量 (CN)

**Files:**
- Modify: `apps/fumadocs/content/docs/cn/server.mdx`

- [ ] **Step 1: 修复环境变量章节**

修复所有 `SB_xxx_yyy` → `SB_xxx__yyy`，包括 OAuth 三级嵌套 `SB_OAUTH__GITHUB__CLIENT_ID`。

- [ ] **Step 2: 修复配置文件环境变量说明**

更新分隔符说明文字为"使用 `__`（双下划线）作为嵌套分隔符"。

---

### Task 4: 修复 agent.mdx 自动更新描述 (CN)

**Files:**
- Modify: `apps/fumadocs/content/docs/cn/agent.mdx`

- [ ] **Step 1: 更新自动更新章节**

将"计划在 P2 阶段实现"的描述替换为已实现的功能说明：

```mdx
## 自动更新

Server 可以向在线 Agent 推送升级命令。触发升级时：

1. Server 发送 `Upgrade` 消息，包含下载 URL 和版本号
2. Agent 下载新版本二进制文件
3. 验证 SHA-256 校验和（通过 `x-checksum-sha256` 响应头）
4. 备份当前二进制文件（`.bak` 后缀）
5. 替换为新二进制
6. 自动重启进程

管理员可以在 Dashboard 的服务器详情页面触发升级，也可以通过 API：

\`\`\`bash
curl -X POST https://your-server/api/servers/{id}/upgrade \
  -H "Cookie: session=..." \
  -H "Content-Type: application/json" \
  -d '{"url": "https://github.com/.../releases/download/v1.2.0/serverbee-agent", "version": "1.2.0"}'
\`\`\`

<Callout type="warn">
自动更新需要 Agent 具有 `upgrade` 能力（CAP_UPGRADE）。默认情况下该能力是关闭的，需要管理员在 Settings → Capabilities 中手动启用。
</Callout>
```

---

### Task 5: 修复 terminal.mdx 审计日志描述 (CN)

**Files:**
- Modify: `apps/fumadocs/content/docs/cn/terminal.mdx`

- [ ] **Step 1: 更新安全说明中的审计日志描述**

将"P2 阶段实现"改为已实现的描述：

```mdx
### 审计日志

终端连接和断开事件会记录到审计日志中。管理员可以在 Settings → Audit Logs 页面查看所有终端会话的访问记录，包括用户、IP 地址和时间戳。
```

- [ ] **Step 2: 添加 Capability 限制说明**

在"限制"章节添加：

```mdx
<Callout type="warn">
Web 终端需要目标服务器启用 `terminal` 能力（CAP_TERMINAL）。如果该能力被禁用，服务器将返回 403 Forbidden。管理员可以在 Settings → Capabilities 中管理此设置。
</Callout>
```

---

### Task 6: 更新 index.mdx 快速链接 (CN)

**Files:**
- Modify: `apps/fumadocs/content/docs/cn/index.mdx`

- [ ] **Step 1: 在快速链接卡片中添加新页面入口**

在现有的 Cards 组件末尾追加新页面链接：

```mdx
<Cards>
  <!-- 保留现有卡片 -->
  <Card title="快速开始" href="/docs/cn/quick-start" />
  <Card title="Server 配置" href="/docs/cn/server" />
  <Card title="Agent 配置" href="/docs/cn/agent" />
  <Card title="监控" href="/docs/cn/monitoring" />
  <Card title="告警与通知" href="/docs/cn/alerts" />
  <Card title="Web 终端" href="/docs/cn/terminal" />
  <Card title="Ping 探测" href="/docs/cn/ping" />
  <Card title="功能开关" href="/docs/cn/capabilities" />
  <Card title="安全设置" href="/docs/cn/security" />
  <Card title="公开状态页" href="/docs/cn/status-page" />
  <Card title="管理员指南" href="/docs/cn/admin" />
  <Card title="API 参考" href="/docs/cn/api-reference" />
  <Card title="架构设计" href="/docs/cn/architecture" />
  <Card title="部署指南" href="/docs/cn/deployment" />
</Cards>
```

- [ ] **Step 7: Commit**

```bash
git add apps/fumadocs/content/docs/cn/
git commit -m "docs(cn): fix env var mapping and update outdated feature descriptions"
```

---

## Chunk 2: 修复现有英文文档过时内容

### Task 7: 修复 terminal.mdx 审计描述 (EN)

**Files:**
- Modify: `apps/fumadocs/content/docs/en/terminal.mdx`

英文版 terminal.mdx 没有明确标记"P2 阶段"，但缺少 capability 限制说明。

- [ ] **Step 1: 添加 capability 限制说明**

在 "Authentication and Access Control" Callout 后添加：

```mdx
<Callout type="warn">
Web terminal access also requires the **terminal** capability (CAP_TERMINAL) to be enabled on the target server. If disabled, the WebSocket upgrade will be rejected with 403 Forbidden. Administrators can manage capabilities in Settings → Capabilities.
</Callout>
```

---

### Task 8: 更新 index.mdx 快速链接 (EN)

**Files:**
- Modify: `apps/fumadocs/content/docs/en/index.mdx`

- [ ] **Step 1: 扩展 Cards 组件添加新页面链接**

```mdx
<Cards>
  <Card title="Quick Start" href="/docs/en/quick-start" />
  <Card title="Server Setup" href="/docs/en/server" />
  <Card title="Agent Setup" href="/docs/en/agent" />
  <Card title="Monitoring" href="/docs/en/monitoring" />
  <Card title="Alerts" href="/docs/en/alerts" />
  <Card title="Web Terminal" href="/docs/en/terminal" />
  <Card title="Ping Monitoring" href="/docs/en/ping" />
  <Card title="Capabilities" href="/docs/en/capabilities" />
  <Card title="Security" href="/docs/en/security" />
  <Card title="Status Page" href="/docs/en/status-page" />
  <Card title="Admin Guide" href="/docs/en/admin" />
  <Card title="API Reference" href="/docs/en/api-reference" />
  <Card title="Architecture" href="/docs/en/architecture" />
  <Card title="Deployment" href="/docs/en/deployment" />
</Cards>
```

- [ ] **Step 2: Commit**

```bash
git add apps/fumadocs/content/docs/en/terminal.mdx apps/fumadocs/content/docs/en/index.mdx
git commit -m "docs(en): add capability warning to terminal and update quick links"
```

---

## Chunk 3: 新增 capabilities.mdx (CN + EN)

### Task 9: 创建 capabilities.mdx (CN)

**Files:**
- Create: `apps/fumadocs/content/docs/cn/capabilities.mdx`

- [ ] **Step 1: 编写中文功能开关文档**

```mdx
---
title: 功能开关
description: 为每台服务器独立控制 Agent 的功能权限，实现最小权限原则。
icon: ToggleRight
---

ServerBee 支持为每台 Agent 独立控制可用功能。通过功能开关（Capability Toggles），管理员可以精确控制每台服务器允许执行的操作，实现最小权限原则。

## 功能列表

ServerBee 定义了 6 个功能位，分为两个风险等级：

### 高风险功能（默认关闭）

| 功能 | 位值 | 说明 |
|------|------|------|
| **Web Terminal** | `CAP_TERMINAL` (1) | 允许通过浏览器打开远程终端 |
| **Remote Exec** | `CAP_EXEC` (2) | 允许远程执行命令 |
| **Auto Upgrade** | `CAP_UPGRADE` (4) | 允许远程推送二进制升级 |

<Callout type="warn">
这三个功能涉及在目标服务器上执行任意代码或替换二进制文件，因此默认关闭。请仅在信任的服务器上启用。
</Callout>

### 低风险功能（默认启用）

| 功能 | 位值 | 说明 |
|------|------|------|
| **ICMP Ping** | `CAP_PING_ICMP` (8) | 允许执行 ICMP 探测任务 |
| **TCP Probe** | `CAP_PING_TCP` (16) | 允许执行 TCP 端口探测任务 |
| **HTTP Probe** | `CAP_PING_HTTP` (32) | 允许执行 HTTP 探测任务 |

新注册的 Agent 默认 capabilities 值为 `56`（即三个 Ping 功能全部启用）。

## 配置方式

### 单台服务器配置

1. 进入 Dashboard → 点击目标服务器 → 服务器详情页
2. 在 **Capabilities** 区域，使用 toggle 开关启用或禁用各项功能
3. 更改立即生效，Server 会通过 WebSocket 实时推送 `CapabilitiesSync` 消息到 Agent

### 批量配置

1. 进入 Settings → Capabilities
2. 搜索或多选服务器
3. 批量启用或禁用指定功能
4. 点击保存，所有选中服务器的功能开关同时更新

### API 配置

单台更新（通过 `PUT /api/servers/{id}`）：

```bash
curl -X PUT https://your-server/api/servers/{id} \
  -H "Cookie: session=..." \
  -H "Content-Type: application/json" \
  -d '{"capabilities": 63}'
```

批量更新：

```bash
curl -X PUT https://your-server/api/servers/batch-capabilities \
  -H "Cookie: session=..." \
  -H "Content-Type: application/json" \
  -d '{"server_ids": ["id1", "id2"], "capabilities": 63}'
```

capabilities 值是各功能位的按位或（OR）结果。例如：
- `56` = ICMP + TCP + HTTP（默认值）
- `63` = 全部功能启用
- `0` = 全部功能禁用

## 双重验证机制

ServerBee 采用 **defense in depth**（纵深防御）策略，在 Server 端和 Agent 端同时验证功能权限：

### Server 端拦截

- **Terminal**：WebSocket 升级请求被 403 拦截
- **Exec**：`POST /api/tasks` 过滤无权限服务器，写入合成结果（`exit_code = -2`，提示 "Capability 'exec' is disabled"）
- **Ping**：`PingService` 按 capability 过滤任务，不向无权限 Agent 同步相关探测任务

### Agent 端拒绝

即使 Server 端消息被绕过，Agent 本地也会检查 capabilities：

- 收到不允许的命令时返回 `CapabilityDenied` 消息
- Server 收到 `CapabilityDenied` 后写入合成结果（`exit_code = -1`）
- 审计日志记录拒绝事件

### 实时同步

当管理员修改 capabilities 后：

1. Server 通过 WebSocket 发送 `CapabilitiesSync` 到目标 Agent
2. Agent 使用 `AtomicU32` 原子更新本地 capabilities 值
3. Server 通过 WebSocket 发送 `CapabilitiesChanged` 到所有连接的浏览器
4. 前端实时更新 UI 状态
5. 如果 Ping 相关 capability 发生变化，Server 自动触发 Ping 任务重同步

## 前端表现

- **Server Detail 页面**：capabilities toggle 区域，在线服务器可实时切换
- **Settings → Capabilities**：批量管理页面，支持搜索和多选
- **Tasks 页面**：无 `CAP_EXEC` 权限的服务器显示为灰色，任务结果标记为 "skipped"
- **Terminal 按钮**：无 `CAP_TERMINAL` 权限的服务器不显示 Terminal 按钮
```

---

### Task 10: 创建 capabilities.mdx (EN)

**Files:**
- Create: `apps/fumadocs/content/docs/en/capabilities.mdx`

- [ ] **Step 1: 编写英文功能开关文档**

内容与中文版对齐，使用英文描述。Frontmatter：

```yaml
---
title: Capabilities
description: Control which features each agent is allowed to use with per-server capability toggles.
icon: ToggleRight
---
```

文档结构与中文版完全一致：
- Capability List (High Risk / Low Risk tables)
- Configuration (Single Server / Batch / API)
- Defense in Depth (Server-side / Agent-side / Real-time Sync)
- Frontend Behavior

- [ ] **Step 2: Commit**

```bash
git add apps/fumadocs/content/docs/cn/capabilities.mdx apps/fumadocs/content/docs/en/capabilities.mdx
git commit -m "docs: add capabilities documentation (CN + EN)"
```

---

## Chunk 4: 新增 security.mdx (CN + EN)

### Task 11: 创建 security.mdx (CN)

**Files:**
- Create: `apps/fumadocs/content/docs/cn/security.mdx`

- [ ] **Step 1: 编写中文安全设置文档**

```mdx
---
title: 安全设置
description: 配置双因素认证、OAuth 登录、密码管理和登录安全策略。
icon: Shield
---

ServerBee 提供多层安全防护，包括双因素认证 (2FA)、OAuth 社交登录、密码策略和登录限流。

## 双因素认证 (2FA)

ServerBee 支持基于 TOTP (Time-based One-Time Password) 的双因素认证，兼容所有标准认证器应用（Google Authenticator、Authy、1Password 等）。

### 启用 2FA

1. 登录后进入 Settings → Security
2. 在 "Two-Factor Authentication" 区域点击 **Setup**
3. 扫描显示的 QR 码（或手动输入 Base32 密钥）
4. 在认证器应用中生成 6 位数验证码
5. 输入验证码点击 **Enable** 完成启用

<Callout type="info">
启用后每次登录都需要输入 6 位 TOTP 验证码。验证码每 30 秒更新一次。
</Callout>

### 禁用 2FA

1. 进入 Settings → Security
2. 点击 **Disable 2FA**
3. 输入当前密码确认身份
4. 2FA 即被禁用

### API 端点

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/auth/2fa/setup` | POST | 生成 TOTP 密钥和 QR 码 |
| `/api/auth/2fa/enable` | POST | 验证码确认后启用 2FA |
| `/api/auth/2fa/disable` | POST | 输入密码后禁用 2FA |
| `/api/auth/2fa/status` | GET | 查询当前 2FA 状态 |

## OAuth 社交登录

ServerBee 支持三种 OAuth 提供商：

| 提供商 | 配置节 | 回调 URL |
|--------|--------|----------|
| GitHub | `[oauth.github]` | `{base_url}/api/auth/oauth/github/callback` |
| Google | `[oauth.google]` | `{base_url}/api/auth/oauth/google/callback` |
| OIDC | `[oauth.oidc]` | `{base_url}/api/auth/oauth/oidc/callback` |

### 配置 OAuth

在 `server.toml` 中添加 OAuth 配置（详见 [Server 配置](/docs/cn/server)）：

```toml
[oauth]
base_url = "https://monitor.example.com"
allow_registration = false  # 是否允许首次 OAuth 登录自动创建用户

[oauth.github]
client_id = "your-github-client-id"
client_secret = "your-github-client-secret"
```

### OAuth 账号管理

- 在 Settings → Security 页面可以查看已关联的 OAuth 账号
- 点击 **Unlink** 可以解除 OAuth 账号关联
- 如果 `allow_registration = false`（默认），OAuth 首次登录不会自动创建新用户，需要管理员先创建用户再关联

### 登录流程

1. 在登录页面点击 OAuth 提供商按钮（如 "Login with GitHub"）
2. 跳转到提供商授权页面
3. 授权后回调到 ServerBee
4. 如果 OAuth 账号已关联现有用户，直接登录
5. 如果未关联且 `allow_registration = true`，自动创建 Member 角色用户并登录
6. 如果未关联且 `allow_registration = false`，返回错误

## 密码管理

### 修改密码

1. 进入 Settings → Security
2. 在 "Change Password" 区域输入当前密码和新密码
3. 点击 **Change Password**

密码使用 argon2 算法哈希存储，符合 OWASP 推荐标准。

### 默认密码提醒

首次部署时如果未设置 `admin.password`，ServerBee 会自动生成随机密码并在启动日志中输出。登录后 Dashboard 顶部会显示醒目提醒横幅，建议立即修改默认密码。

## 登录安全

### 登录限流

ServerBee 对登录端点实施 IP 级别的速率限制：

- 默认每 15 分钟窗口内最多 **5 次** 失败尝试（可通过 `rate_limit.login_max` 配置）
- 超过限制后返回 429 Too Many Requests
- 过期的限流记录由后台 session_cleaner 任务自动清理

### Agent 注册限流

Agent 注册端点同样受限：

- 默认每 15 分钟窗口内最多 **3 次** 注册尝试（可通过 `rate_limit.register_max` 配置）

### Session 安全

- Session Cookie 默认设置 `HttpOnly` + `Secure` 标志
- Session 有效期 24 小时（可配置 `auth.session_ttl`）
- 开发环境可通过 `auth.secure_cookie = false` 关闭 Secure 标志
```

---

### Task 12: 创建 security.mdx (EN)

**Files:**
- Create: `apps/fumadocs/content/docs/en/security.mdx`

- [ ] **Step 1: 编写英文安全设置文档**

Frontmatter：

```yaml
---
title: Security
description: Configure two-factor authentication, OAuth login, password management, and login security policies.
icon: Shield
---
```

文档结构与中文版完全一致：Two-Factor Authentication / OAuth Login / Password Management / Login Security

- [ ] **Step 2: Commit**

```bash
git add apps/fumadocs/content/docs/cn/security.mdx apps/fumadocs/content/docs/en/security.mdx
git commit -m "docs: add security settings documentation (CN + EN)"
```

---

## Chunk 5: 新增 status-page.mdx (CN + EN)

### Task 13: 创建 status-page.mdx (CN)

**Files:**
- Create: `apps/fumadocs/content/docs/cn/status-page.mdx`

- [ ] **Step 1: 编写中文公开状态页文档**

```mdx
---
title: 公开状态页
description: 为访客提供无需登录的服务器在线状态展示页面。
icon: Globe
---

ServerBee 提供一个公开状态页面，无需登录即可查看服务器的在线状态和基本指标。适合向用户或团队展示服务运行状况。

## 访问方式

状态页面地址：`https://your-server/status`

该页面不需要任何认证，任何人都可以访问。

## 页面内容

状态页展示以下信息：

- **在线/总数统计**：显示当前在线服务器数量和总服务器数量
- **服务器列表**：按分组展示所有非隐藏的服务器
- **在线状态**：每台服务器显示在线/离线状态指示
- **实时指标**：在线服务器显示 CPU、内存、磁盘使用率进度条
- **自动刷新**：页面每 10 秒自动从 API 获取最新数据

## 显示规则

- 仅展示 **非隐藏** 的服务器（管理员可在服务器编辑中设置 `hidden` 属性）
- 按服务器分组（Group）组织展示
- 离线服务器不显示指标数据，仅显示离线状态

## API 端点

```
GET /api/status
```

公开端点，无需认证。返回结构：

```json
{
  "data": {
    "servers": [
      {
        "id": "server-uuid",
        "name": "Web Server 1",
        "hostname": "web1.example.com",
        "is_online": true,
        "group_name": "Production",
        "cpu": 45.2,
        "mem_used": 8589934592,
        "mem_total": 17179869184,
        "disk_used": 53687091200,
        "disk_total": 107374182400
      }
    ],
    "groups": [
      { "id": "group-uuid", "name": "Production" }
    ],
    "online_count": 8,
    "total_count": 10
  }
}
```

## 隐藏服务器

如果某些服务器不希望出现在状态页上：

1. 进入服务器详情页 → 编辑
2. 勾选 **Hidden** 选项
3. 该服务器不会出现在 `/api/status` 返回的列表中
```

---

### Task 14: 创建 status-page.mdx (EN)

**Files:**
- Create: `apps/fumadocs/content/docs/en/status-page.mdx`

- [ ] **Step 1: 编写英文公开状态页文档**

Frontmatter：

```yaml
---
title: Status Page
description: A public page showing server online status without authentication.
icon: Globe
---
```

内容与中文版对齐。

- [ ] **Step 2: Commit**

```bash
git add apps/fumadocs/content/docs/cn/status-page.mdx apps/fumadocs/content/docs/en/status-page.mdx
git commit -m "docs: add public status page documentation (CN + EN)"
```

---

## Chunk 6: 新增 admin.mdx (CN + EN)

### Task 15: 创建 admin.mdx (CN)

**Files:**
- Create: `apps/fumadocs/content/docs/cn/admin.mdx`

- [ ] **Step 1: 编写中文管理员指南文档**

```mdx
---
title: 管理员指南
description: 用户管理、审计日志、远程命令和计费信息管理。
icon: UserCog
---

本页介绍仅管理员（Admin 角色）可使用的功能。

## 用户管理

ServerBee 支持多用户，分为两种角色：

| 角色 | 权限 |
|------|------|
| **Admin** | 完全管理权限：用户管理、服务器配置、告警规则、通知渠道、审计日志等 |
| **Member** | 只读权限：查看 Dashboard、服务器详情、Ping 结果 |

### 管理用户

进入 Settings → Users 页面：

- **创建用户**：输入用户名、密码和角色
- **编辑角色**：修改用户的角色（Admin/Member）
- **删除用户**：删除用户账号（禁止删除最后一个 Admin）

### API 端点

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/users` | GET | 列出所有用户 |
| `/api/users` | POST | 创建用户 |
| `/api/users/{id}` | GET | 获取用户详情 |
| `/api/users/{id}` | PUT | 更新用户角色 |
| `/api/users/{id}` | DELETE | 删除用户 |

## 审计日志

ServerBee 自动记录关键操作的审计日志，帮助管理员追踪安全事件。

### 记录的事件

- 用户登录（成功/失败）
- 密码修改
- 2FA 启用/禁用
- 终端连接/断开
- Capability 拒绝事件

### 查看审计日志

进入 Settings → Audit Logs 页面，可以：

- 浏览所有审计记录（分页显示）
- 查看用户、操作类型、详情、IP 地址和时间

### API 端点

```
GET /api/audit-logs?limit=50&offset=0
```

返回审计日志列表，支持分页参数 `limit` 和 `offset`。每条记录包含：

```json
{
  "id": 1,
  "user_id": "user-uuid",
  "action": "login",
  "detail": "Login successful",
  "ip": "192.168.1.100",
  "created_at": "2026-03-14T10:30:00Z"
}
```

审计日志默认保留 180 天（可通过 `retention.audit_logs_days` 配置）。

## 远程命令

管理员可以向在线服务器下发远程命令并获取执行结果。

### 使用方式

1. 进入 Settings → Tasks 页面
2. 输入要执行的命令
3. 选择目标服务器（可多选）
4. 点击 **Execute**

### 执行流程

1. Server 创建 Task 记录
2. 通过 WebSocket 向每台目标 Agent 发送执行命令
3. Agent 执行命令，将 stdout/stderr 和 exit_code 回写
4. 结果存储在 `task_results` 表中

### 限制

- 命令执行超时：300 秒
- 需要目标服务器启用 `exec` 能力（CAP_EXEC）
- 未启用 CAP_EXEC 的服务器会收到合成结果：`exit_code = -2`，提示功能被禁用
- 仅 Admin 角色可以创建任务

### API 端点

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/tasks` | POST | 创建并下发命令 |
| `/api/tasks/{id}` | GET | 获取任务详情 |
| `/api/tasks/{id}/results` | GET | 获取执行结果 |

创建任务请求体：

```json
{
  "command": "uptime",
  "server_ids": ["server-id-1", "server-id-2"],
  "timeout": 60
}
```

## 计费信息

管理员可以为每台服务器记录计费相关信息，方便追踪 VPS 费用和到期时间。

### 管理计费信息

1. 进入服务器详情页 → 点击编辑按钮
2. 在 "Billing" 区域填写以下信息：

| 字段 | 说明 |
|------|------|
| `price` | 价格 |
| `billing_cycle` | 计费周期（monthly/quarterly/yearly 等） |
| `currency` | 货币单位（CNY/USD 等） |
| `expired_at` | 到期时间 |
| `traffic_limit` | 流量限额（字节） |

### 到期告警

可以创建 `expiration` 类型的告警规则，在服务器到期前 N 天自动发送通知：

1. 进入 Settings → Alerts
2. 创建新规则，指标类型选择 **expiration**
3. 设置阈值为提前提醒天数（如 7 表示到期前 7 天告警）
4. 关联通知组

### 流量告警

使用 `transfer_in_cycle` / `transfer_out_cycle` / `transfer_all_cycle` 告警类型，可以监控当前计费周期内的累计流量是否超过设定阈值。详见 [告警与通知](/docs/cn/alerts)。
```

---

### Task 16: 创建 admin.mdx (EN)

**Files:**
- Create: `apps/fumadocs/content/docs/en/admin.mdx`

- [ ] **Step 1: 编写英文管理员指南文档**

Frontmatter：

```yaml
---
title: Admin Guide
description: User management, audit logs, remote commands, and billing management.
icon: UserCog
---
```

文档结构与中文版完全一致。

- [ ] **Step 2: Commit**

```bash
git add apps/fumadocs/content/docs/cn/admin.mdx apps/fumadocs/content/docs/en/admin.mdx
git commit -m "docs: add admin guide documentation (CN + EN)"
```

---

## Chunk 7: 新增 api-reference.mdx (CN + EN)

### Task 17: 创建 api-reference.mdx (CN)

**Files:**
- Create: `apps/fumadocs/content/docs/cn/api-reference.mdx`

- [ ] **Step 1: 编写中文 API 参考文档**

```mdx
---
title: API 参考
description: ServerBee REST API 概览、认证方式和 Swagger UI 交互式文档。
icon: FileCode
---

ServerBee 提供完整的 REST API，支持所有 Dashboard 功能的程序化访问。所有 API 均通过 OpenAPI 3.0 规范文档化。

## Swagger UI

ServerBee 内置 Swagger UI 交互式 API 文档：

```
https://your-server/swagger-ui/
```

你可以在 Swagger UI 中浏览所有 50+ 个 API 端点、查看请求/响应模型、直接发送测试请求。

## 认证方式

ServerBee API 支持两种认证方式：

### Session Cookie

浏览器登录后自动使用。调用 `/api/auth/login` 获取 session：

```bash
curl -X POST https://your-server/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "your-password"}' \
  -c cookies.txt

# 后续请求带上 cookie
curl https://your-server/api/servers -b cookies.txt
```

### API Key

适合自动化场景。在 Settings → API Keys 页面创建：

```bash
curl https://your-server/api/servers \
  -H "X-API-Key: sb_your-api-key-here"
```

API Key 格式为 `sb_` 前缀 + 43 字符随机字符串，创建时仅显示一次。

## 端点概览

### 公开端点（无需认证）

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/auth/login` | 用户登录 |
| GET | `/api/auth/oauth/{provider}` | OAuth 授权跳转 |
| GET | `/api/auth/oauth/{provider}/callback` | OAuth 回调 |
| GET | `/api/status` | 公开状态页数据 |

### 认证端点（Session 或 API Key）

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/auth/logout` | 用户登出 |
| GET | `/api/auth/me` | 获取当前用户信息 |
| POST | `/api/auth/change-password` | 修改密码 |
| GET/POST | `/api/auth/2fa/*` | 2FA 管理 |
| GET/DELETE | `/api/auth/oauth/accounts` | OAuth 账号管理 |
| GET | `/api/servers` | 列出服务器 |
| GET | `/api/servers/{id}` | 获取服务器详情 |
| GET | `/api/servers/{id}/records` | 获取指标记录 |
| GET | `/api/servers/{id}/gpu-records` | 获取 GPU 记录 |
| GET | `/api/server-groups` | 列出服务器分组 |
| GET | `/api/ping-tasks` | 列出 Ping 任务 |
| GET | `/api/ping-tasks/{id}/records` | 获取 Ping 记录 |

### 管理员端点（需要 Admin 角色）

| 方法 | 路径 | 说明 |
|------|------|------|
| POST/PUT/DELETE | `/api/servers/*` | 服务器管理 |
| PUT | `/api/servers/batch-capabilities` | 批量更新功能开关 |
| POST | `/api/servers/{id}/upgrade` | 触发 Agent 升级 |
| CRUD | `/api/server-groups/*` | 服务器分组管理 |
| CRUD | `/api/notifications/*` | 通知渠道管理 |
| CRUD | `/api/notification-groups/*` | 通知组管理 |
| CRUD | `/api/alert-rules/*` | 告警规则管理 |
| CRUD | `/api/ping-tasks/*` | Ping 任务管理 |
| POST | `/api/tasks` | 创建远程命令任务 |
| GET | `/api/tasks/{id}` | 获取任务详情和结果 |
| CRUD | `/api/users/*` | 用户管理 |
| GET | `/api/audit-logs` | 审计日志 |
| GET/PUT | `/api/settings/*` | 系统设置 |
| POST | `/api/settings/backup` | 数据库备份 |
| POST | `/api/settings/restore` | 数据库恢复 |

### WebSocket 端点

| 路径 | 说明 |
|------|------|
| `/api/ws/browser` | 浏览器实时数据推送 |
| `/api/ws/terminal/{server_id}` | Web 终端代理 |

## 错误响应

所有 API 错误返回统一格式：

```json
{
  "error": "Error message describing what went wrong"
}
```

常见状态码：

| 状态码 | 说明 |
|--------|------|
| 400 | 请求参数错误 |
| 401 | 未认证 |
| 403 | 无权限（角色不足或功能被禁用） |
| 404 | 资源不存在 |
| 429 | 请求过于频繁（限流） |
| 500 | 服务器内部错误 |
```

---

### Task 18: 创建 api-reference.mdx (EN)

**Files:**
- Create: `apps/fumadocs/content/docs/en/api-reference.mdx`

- [ ] **Step 1: 编写英文 API 参考文档**

Frontmatter：

```yaml
---
title: API Reference
description: ServerBee REST API overview, authentication, and Swagger UI interactive docs.
icon: FileCode
---
```

文档结构与中文版完全一致。

- [ ] **Step 2: Commit**

```bash
git add apps/fumadocs/content/docs/cn/api-reference.mdx apps/fumadocs/content/docs/en/api-reference.mdx
git commit -m "docs: add API reference documentation (CN + EN)"
```

---

## Chunk 8: 更新导航 + 构建验证

### Task 19: 更新 meta.json 导航 (CN + EN)

**Files:**
- Modify: `apps/fumadocs/content/docs/cn/meta.json`
- Modify: `apps/fumadocs/content/docs/en/meta.json`

- [ ] **Step 1: 更新中文 meta.json**

```json
{
  "title": "文档",
  "pages": [
    "index",
    "---快速开始---",
    "quick-start",
    "---指南---",
    "server",
    "agent",
    "configuration",
    "---功能---",
    "monitoring",
    "alerts",
    "terminal",
    "ping",
    "capabilities",
    "status-page",
    "---管理---",
    "security",
    "admin",
    "api-reference",
    "---参考---",
    "architecture",
    "deployment"
  ]
}
```

- [ ] **Step 2: 更新英文 meta.json**

```json
{
  "title": "Documentation",
  "pages": [
    "index",
    "---Quick Start---",
    "quick-start",
    "---Guides---",
    "server",
    "agent",
    "configuration",
    "---Features---",
    "monitoring",
    "alerts",
    "terminal",
    "ping",
    "capabilities",
    "status-page",
    "---Administration---",
    "security",
    "admin",
    "api-reference",
    "---Reference---",
    "architecture",
    "deployment"
  ]
}
```

---

### Task 20: 构建验证

**Files:**
- Verify: `apps/fumadocs/` (TypeScript + MDX build)

- [ ] **Step 1: 运行类型检查**

```bash
cd apps/fumadocs && bun run types:check
```

Expected: 无 TypeScript 错误

- [ ] **Step 2: 运行 lint**

```bash
cd apps/fumadocs && bun run lint
```

Expected: 无 Biome 错误

- [ ] **Step 3: 运行构建**

```bash
cd apps/fumadocs && bun run build
```

Expected: 构建成功，所有 MDX 文件被正确处理

- [ ] **Step 4: Commit meta.json + PROGRESS.md 更新**

```bash
git add apps/fumadocs/content/docs/cn/meta.json apps/fumadocs/content/docs/en/meta.json
git commit -m "docs: update navigation with new documentation pages"
```

---

### Task 21: 更新 PROGRESS.md

**Files:**
- Modify: `docs/superpowers/plans/PROGRESS.md`

- [ ] **Step 1: 更新 P3-f T5 状态**

将 `| T5 | Fumadocs 文档站内容 | **跳过** |` 改为 `| T5 | Fumadocs 文档站内容 | **done** |`

- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/plans/PROGRESS.md
git commit -m "docs: mark Fumadocs documentation as complete in PROGRESS.md"
```
