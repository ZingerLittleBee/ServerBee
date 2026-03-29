# iOS MVP Design Spec

**Date**: 2026-03-29
**Branch**: spokane
**Status**: Approved

## Overview

ServerBee iOS 原生客户端 MVP，目标是提供核心监控能力：认证、服务器列表与实时状态、告警查看、APNs 推送通知。采用"后端先行，逐层推进"的实现方案。

### MVP 范围

1. 服务端 Mobile Auth API（JWT-like token 对）
2. QR 扫码一键登录
3. 核心监控（服务器列表 + 实时状态 + 详情 + 历史图表）— iOS 端已基本完成
4. 告警 + APNs 原生推送
5. 本地 Xcode 构建，手动 TestFlight 分发

### 不在 MVP 范围

- Ping/网络监控页面
- Docker 管理
- 文件管理
- 终端
- CI 自动化构建
- App Store 上架

---

## Section 1: 服务端 Mobile Auth API

### 数据库

#### 新增 `mobile_session` 表

| 列 | 类型 | 说明 |
|---|---|---|
| id | TEXT PK | UUID |
| user_id | TEXT FK | 关联 user |
| refresh_token_hash | TEXT | argon2 hash of refresh token |
| installation_id | TEXT | 设备标识（iOS UUID） |
| device_name | TEXT | 设备描述（如 "iPhone 15"） |
| created_at | DATETIME | 创建时间 |
| expires_at | DATETIME | 刷新 token 过期时间 |
| last_used_at | DATETIME | 最近使用时间 |

#### 修改 `session` 表

新增两列：

| 列 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| source | TEXT | `"web"` | `"web"` 或 `"mobile"` |
| mobile_session_id | TEXT NULL | NULL | 仅 `source="mobile"` 时有值，FK → `mobile_session.id` |

现有记录全部默认 `source="web"`, `mobile_session_id=NULL`。

`mobile_session_id` 是关键的桥接列——当 Bearer token 命中 session 表时，通过此列可以找到对应的 `mobile_session` 行，从而实现：
- **Logout**: Bearer token → session.mobile_session_id → 删除 mobile_session + session
- **last_used_at**: 每次 validate_session 成功时，更新 mobile_session.last_used_at
- **Push 注册**: device_token.mobile_session_id 从 session.mobile_session_id 获取
- **设备管理**: 远程注销某设备时，删除 mobile_session + 关联的所有 session 行

### Token 方案

不引入 `jsonwebtoken` 新依赖。Access token 采用与现有 session 表一致的 opaque token 方案——生成随机 token 存入 `session` 表，通过 `source` 字段区分来源。Refresh token 是独立的随机 token，hash 后存入 `mobile_session` 表。

### Token 生命周期

- Access token: 15 分钟（`access_expires_in_secs: 900`）
- Refresh token: 30 天（`refresh_expires_in_secs: 2592000`）
- Refresh 时签发全新 token 对（rotation），旧 refresh token 立即失效

### 端点

| 端点 | 方法 | 认证 | 说明 |
|---|---|---|---|
| `/api/mobile/auth/login` | POST | 无 | 用户名密码登录，返回 access_token + refresh_token |
| `/api/mobile/auth/refresh` | POST | 无 | 用 refresh_token 换新的 token 对 |
| `/api/mobile/auth/logout` | POST | Bearer | 注销，删除 refresh_token + access_token |
| `/api/mobile/auth/devices` | GET | Session/API Key/Bearer | 列出已登录的移动设备 |
| `/api/mobile/auth/devices/{id}` | DELETE | Session/API Key/Bearer | 远程注销某设备 |

#### Login 请求/响应

请求：
```json
{
  "username": "admin",
  "password": "...",
  "installation_id": "uuid-of-device",
  "device_name": "iPhone 15",
  "totp_code": "123456"
}
```

响应（200）：
```json
{
  "data": {
    "access_token": "random-opaque-token",
    "access_expires_in_secs": 900,
    "refresh_token": "random-opaque-token",
    "refresh_expires_in_secs": 2592000,
    "token_type": "Bearer",
    "user": {
      "id": "uuid",
      "username": "admin",
      "role": "admin"
    }
  }
}
```

错误码：401 凭证错误，422 需要 2FA，429 请求过多。

#### Refresh 请求

```json
{
  "refresh_token": "...",
  "installation_id": "uuid-of-device"
}
```

校验 refresh_token hash + installation_id 匹配 + 未过期。成功后删除旧 session + 旧 refresh token hash，签发全新 token 对。

### 配置

`AppConfig` 新增 `mobile` section：

```toml
[mobile]
access_ttl = 900        # 15 min (seconds)
refresh_ttl = 2592000   # 30 days (seconds)
```

环境变量：`SERVERBEE_MOBILE__ACCESS_TTL`，`SERVERBEE_MOBILE__REFRESH_TTL`。

按照项目惯例（CLAUDE.md: "When adding/changing env vars, update ENV.md and docs simultaneously"），需同步更新：
- `ENV.md` — 新增 `SERVERBEE_MOBILE__ACCESS_TTL` 和 `SERVERBEE_MOBILE__REFRESH_TTL`
- `apps/docs/content/docs/{en,cn}/configuration.mdx` — 新增 `[mobile]` section 文档

### 限流

Mobile login 复用现有 `login_rate_limit`（15 分钟窗口内最多 N 次）。

---

## Section 2: QR 扫码配对登录

### 流程

```
Web 端（已登录）                    服务端                         iOS 端
     │                              │                              │
     ├── POST /api/mobile/pair ────►│                              │
     │                              ├── 生成 pairing_code          │
     │◄── { code, expires_at } ─────┤   存入 DashMap（5min TTL）   │
     │                              │                              │
     │  展示 QR Code                │                              │
     │  (内容: JSON {               │                              │
     │    type, server_url, code }) │                              │
     │                              │                              │
     │                              │    ◄── POST /api/mobile/auth/pair ──┤
     │                              │        { code, installation_id,     │
     │                              │          device_name }              │
     │                              ├── 验证 code                  │
     │                              ├── 签发 token 对              │
     │                              │── { access_token,            │
     │                              │    refresh_token, user } ───►│
     │                              │                              │
     │                              ├── 删除 pairing_code          │
     │                              │                              │
```

### 端点

| 端点 | 方法 | 认证 | 说明 |
|---|---|---|---|
| `/api/mobile/pair` | POST | 需登录 | Web 端生成配对码 |
| `/api/mobile/auth/pair` | POST | 无需认证 | iOS 扫码兑换 token 对 |

#### 生成配对码响应

```json
{
  "data": {
    "code": "sb_pair_aBcDeFgH...",
    "expires_in_secs": 300
  }
}
```

#### 兑换请求

```json
{
  "code": "sb_pair_aBcDeFgH...",
  "installation_id": "uuid-of-device",
  "device_name": "iPhone 15"
}
```

响应与 login 相同（`MobileTokenResponse`）。

### 配对码设计

- 格式：`sb_pair_` 前缀 + 32 字节随机 base64url 编码
- 存储：`AppState` 新增 `pending_pairs: DashMap<String, PendingPair>`
- `PendingPair` 结构：`{ user_id: String, created_at: DateTime<Utc> }`
- TTL：5 分钟，过期清除（懒清理 + 定期清理）
- 一次性：兑换成功后立即删除
- 每个用户同时最多一个有效配对码（生成新码时删除旧码）

### QR Code 内容

```json
{
  "type": "serverbee_pair",
  "server_url": "https://my-server.example.com:9527",
  "code": "sb_pair_aBcDeFgH..."
}
```

`type` 字段用于 iOS 端校验。`server_url` 由 Web 端从当前 `window.location.origin` 获取。

### Web 端 UI

设置页新增"移动设备"板块：
- "添加设备"按钮 → 调用 `/api/mobile/pair` → 用 `qrcode` npm 包生成 QR 展示
- 显示 5 分钟倒计时，过期可重新生成
- 已配对设备列表（来自 `/api/mobile/auth/devices`），显示设备名、最近活跃时间
- 支持远程注销

### iOS 端

- 登录页新增"扫码登录"按钮
- `AVCaptureSession` 扫描二维码
- 解析 JSON → 校验 `type == "serverbee_pair"` → 提取 `server_url` + `code`
- 调用 `/api/mobile/auth/pair` 完成认证
- `Info.plist` 添加 `NSCameraUsageDescription`

---

## Section 3: Auth Middleware 扩展

### HTTP Auth Middleware

修改 `middleware/auth.rs` 的 `auth_middleware`，新增 Bearer token 路径。三条路径按优先级：

1. Session Cookie → `validate_session()`
2. `X-API-Key` header → `validate_api_key()`
3. `Authorization: Bearer <token>` header → `validate_session()`（复用，token 存在 session 表）

新增辅助函数：

```rust
fn extract_bearer_token(req: &Request) -> Option<String> {
    req.headers()
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|s| s.to_string())
}
```

Bearer token 本质上也是 session token（存在 `session` 表里），只是 `source = "mobile"`。`validate_session` 不需要改动——它只校验 token 是否存在且未过期。

### WebSocket 认证

`router/ws/browser.rs` 的 `validate_browser_auth` 同样新增 Bearer token 路径：

```rust
// 现有: Session cookie → API key
// 新增: Bearer token
if let Some(token) = extract_bearer_token(headers)
    && let Ok(Some(user)) =
        AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl).await
{
    return Some(user.id);
}
```

iOS 的 `WebSocketClient` 已经在连接时设置了 `Authorization: Bearer <token>` header，所以 iOS 端不需要改动。

### Access Token TTL

Mobile access token 的 TTL（15 分钟）独立于 Web session TTL（24 小时）。`validate_session` 当前接收单一 `session_ttl` 参数，需要重构为：先查 session 记录，读取 `source` 字段，然后根据来源选择对应 TTL 再校验过期。函数签名改为接收 `&AppConfig`（或两个 TTL 参数）：
- `source == "web"` → `config.auth.session_ttl`
- `source == "mobile"` → `config.mobile.access_ttl`

### validate_session 调用点完整清单

`validate_session` 签名变更后，以下所有调用点必须同步更新：

1. `crates/server/src/middleware/auth.rs` — HTTP auth middleware
2. `crates/server/src/router/ws/browser.rs:41` — Browser WebSocket 认证
3. `crates/server/src/router/ws/terminal.rs:75` — Terminal WebSocket 认证
4. `crates/server/src/router/ws/docker_logs.rs:67` — Docker logs WebSocket 认证
5. `crates/server/src/service/auth.rs` — 函数定义本身 + 2 个单元测试（`test_validate_session_valid`, `test_validate_session_invalid_token`）

此外 `crates/server/src/entity/session.rs` 需要新增 `source` 和 `mobile_session_id` 字段到 `Model` struct。

terminal.rs 和 docker_logs.rs 各有自己的 `validate_auth` + `extract_session_cookie` + `extract_api_key` 辅助函数（与 browser.rs 重复）。Bearer token 支持必须同时添加到这三个 WS handler 中。实现时考虑将公共认证逻辑抽取到共享模块以消除重复。

---

## Section 4: APNs 推送通知

### 架构

```
告警触发 → AlertService → NotificationService::dispatch()
                                    │
                                    ├── Webhook / Telegram / Bark / Email（现有）
                                    └── APNs（新增）
```

APNs 作为新增通知渠道类型，完全融入现有 Notification 体系。

### 新增通知渠道类型

`ChannelConfig` 枚举新增 `Apns` 变体：

```rust
Apns {
    key_id: String,        // Apple Developer Key ID (10 chars)
    team_id: String,       // Apple Developer Team ID
    private_key: String,   // .p8 文件内容 (base64 PEM)
    bundle_id: String,     // com.serverbee.mobile
    sandbox: bool,         // true = 开发环境, false = 生产环境
}
```

### Rust 依赖

新增 `a2` crate — Apple APNs HTTP/2 客户端。封装了 JWT 签名、连接池、HTTP/2 长连接。

### 数据库

#### 新增 `device_token` 表

| 列 | 类型 | 说明 |
|---|---|---|
| id | TEXT PK | UUID |
| user_id | TEXT FK | 关联 user |
| mobile_session_id | TEXT FK | 关联 mobile_session |
| token | TEXT UNIQUE | APNs device token (hex string) |
| created_at | DATETIME | |
| updated_at | DATETIME | |

### 端点

| 端点 | 方法 | 认证 | 说明 |
|---|---|---|---|
| `/api/mobile/push/register` | POST | Bearer | 注册/更新 device token |
| `/api/mobile/push/unregister` | POST | Bearer | 注销 device token |

#### Register 请求

```json
{
  "device_token": "hex-encoded-apns-token"
}
```

如果 token 已存在（同一设备重新登录），更新关联的 `mobile_session_id` 和 `updated_at`。

### 推送触发逻辑

在 `NotificationService::dispatch` 中，当 `notify_type == "apns"` 时：

1. 解析 APNs 配置（key_id, team_id, private_key）
2. 查询需要推送的 device tokens：
   - `device_token` 表按 `user_id` 索引
   - dispatch 时通过 `mobile_session.user_id` → `device_token.user_id` 查询目标设备
   - APNs 渠道与其他渠道一样归属通知组，告警规则绑定通知组触发 dispatch
   - 推送目标范围：查询 `device_token` 表中**所有用户**的有效 token（ServerBee 是小型自托管工具，通常 1-2 个管理员，全量推送合理）
   - 这意味着一个 APNs 通知渠道配置即可覆盖所有已注册设备，无需按用户创建多个渠道
3. 用 `a2::Client` 构建 `Notification` 并发送
4. 处理 APNs 响应：invalid token 时自动清理 `device_token` 记录

### 推送 Payload

```json
{
  "aps": {
    "alert": {
      "title": "[ServerBee] my-vps 告警",
      "body": "CPU 使用率 > 90% 持续 5 分钟"
    },
    "sound": "default",
    "badge": 1
  },
  "server_id": "xxx",
  "rule_id": "yyy"
}
```

Title 和 body 使用 `NotifyContext` 模板渲染，复用现有 `DEFAULT_TEMPLATE` 的信息。自定义 payload 携带 `server_id` 和 `rule_id`，iOS 端收到推送后点击可跳转到对应服务器详情页。

### 简化决策

- 不做静默推送——MVP 只做可见告警通知
- 不做精细 badge 计数——固定 badge = 1，用户打开 App 后清零
- APNs 连接不常驻——每次 dispatch 时创建连接（`a2` 有连接池），避免增加 AppState 复杂度

---

## Section 5: iOS 客户端调整

### 5.1 新增：扫码登录页面

- `Views/Auth/QRScannerView.swift`：基于 `AVCaptureSession` 的二维码扫描视图
- `LoginView` 新增"扫码登录"按钮，点击后 present `QRScannerView`
- 扫码解析 JSON → 校验 `type == "serverbee_pair"` → 提取 `server_url` + `code` → 调用 `/api/mobile/auth/pair`

### 5.2 新增：推送通知集成

- `Services/PushNotificationManager.swift`：
  - 请求通知权限（`UNUserNotificationCenter.requestAuthorization`）
  - 注册 APNs → 获取 device token → 上报 `/api/mobile/push/register`
  - 处理推送点击 → 解析 `server_id` → 发布 deep link 导航
- `ServerBeeApp.swift` 添加 `@UIApplicationDelegateAdaptor` 处理 APNs 回调
- `ContentView` 监听 deep link，收到推送跳转到对应 `ServerDetailView`

### 5.3 修正：APIClient 解包统一

当前 `APIClient.request` 返回原始 `T`，调用方手动解 `ApiResponse<T>.data`。需要统一：
- `APIClient.request` 内部自动 unwrap `ApiResponse<T>.data`，返回 `T`
- 与 Web 端 `api-client.ts` 行为一致

**影响的调用方（必须同步修改）**：
- `ServersViewModel.fetchServers` — 当前 `let response: ApiResponse<[ServerStatus]> = try await apiClient.get(...); servers = response.data`
- `ServerDetailViewModel.fetchDetail` — 同上模式
- `ServerDetailViewModel.fetchRecords` — 同上模式
- `AlertsViewModel.fetchEvents` — 同上模式
- `AlertDetailViewModel.fetchDetail` — 同上模式

所有这些调用方需改为直接 `let servers: [ServerStatus] = try await apiClient.get(...)`，去掉 `.data` 解包。

### 5.3.1 新增：告警详情端点

`AlertDetailViewModel` 调用 `/api/mobile/alerts/{alert_key}`，但服务端目前没有此端点。需要在 `router/api/mobile.rs` 中新增：

| 端点 | 方法 | 认证 | 说明 |
|---|---|---|---|
| `/api/mobile/alerts/{alert_key}` | GET | Bearer/Session/API Key | 返回告警详情，组合 alert_state + alert_rule 数据 |

响应体对应 iOS 端 `MobileAlertDetail` 模型（包含 rule_enabled、rule_trigger_mode 等 alert_state 表没有的字段）。

或者，将此路径放在现有 `/api/alert-events` 体系下（如 `/api/alert-events/{alert_key}`），与其他 read-only 路由一起，无需 mobile 前缀。

### 5.4 Xcode 项目配置更新

**Info.plist**:
- `NSCameraUsageDescription`: "ServerBee needs camera access to scan QR codes for quick login"

**Push Notification Entitlement**:
`aps-environment` 不是 Info.plist 键，而是必须通过 entitlements 文件配置。需要：
- 新增 `apps/ios/ServerBee/ServerBee.entitlements` 文件，包含 `aps-environment` = `development`（TestFlight/生产时改为 `production`）
- 更新 `apps/ios/project.yml` 的 target settings，添加 `CODE_SIGN_ENTITLEMENTS: ServerBee/ServerBee.entitlements`
- 在 XcodeGen target 中启用 Push Notifications capability

### 5.5 无需改动的模块

以下模块 iOS 端已实现且与服务端现有 REST/WS API 兼容（解包修正后）：
- `ServerStatus` 模型 → `/api/servers`
- `BrowserMessage` WebSocket 模型 → 服务端 `BrowserMessage`
- `WebSocketClient` → 已设置 `Authorization: Bearer` header
- Views 层 — UI 代码不需改动，数据流由 ViewModel 处理

**注意**：`AlertModels` 中的 `MobileAlertEvent` 兼容现有 `/api/alert-events` 响应（`AlertEventResponse`），但 `MobileAlertDetail` 需要新增服务端端点（见 5.3.1）。

### 5.6 Web 端改动

设置页新增"移动设备管理"板块：
- "添加设备"按钮 → 生成 QR code 弹窗（`qrcode` npm 包）
- 已配对设备列表（`/api/mobile/auth/devices`），显示设备名、最近活跃时间
- 远程注销功能

通知渠道设置新增 "Apple Push Notification" 类型选项：
- 配置表单：Key ID、Team ID、上传 .p8 文件、Bundle ID、sandbox 开关
- 测试发送功能（复用现有 notification test 逻辑）

---

## 实现顺序

1. **服务端 Mobile Auth API** — migration + 端点 + auth middleware 扩展
2. **iOS 联调** — Bearer token 认证跑通，核心监控验证
3. **QR 扫码配对** — 服务端端点 + Web 端 UI + iOS 扫码页
4. **APNs 推送** — `device_token` 表 + 注册端点 + `a2` 集成 + iOS 推送注册
5. **收尾** — APIClient 统一、测试、Web 端设备管理 UI

---

## 新增文件清单

### 服务端 (Rust)

- `crates/server/src/migration/m20260329_000001_create_mobile_session.rs`
- `crates/server/src/migration/m20260329_000002_add_session_source.rs`
- `crates/server/src/migration/m20260329_000003_create_device_token.rs`
- `crates/server/src/entity/mobile_session.rs`
- `crates/server/src/entity/device_token.rs`
- `crates/server/src/router/api/mobile.rs` — mobile auth + push + pair 端点
- `crates/server/src/service/mobile_auth.rs` — token 签发/验证/刷新逻辑
- `crates/server/src/service/apns.rs` — APNs 推送发送

### 修改文件 (Rust)

- `crates/server/src/entity/mod.rs` — 注册新 entity（mobile_session, device_token）
- `crates/server/src/entity/session.rs` — Model struct 新增 `source`, `mobile_session_id` 字段
- `crates/server/src/migration/mod.rs` — 注册新 migration
- `crates/server/src/router/api/mod.rs` — 挂载 mobile router
- `crates/server/src/middleware/auth.rs` — 新增 Bearer token 路径
- `crates/server/src/router/ws/browser.rs` — WS 新增 Bearer token 认证
- `crates/server/src/router/ws/terminal.rs` — WS 新增 Bearer token 认证
- `crates/server/src/router/ws/docker_logs.rs` — WS 新增 Bearer token 认证
- `crates/server/src/state.rs` — 新增 `pending_pairs` DashMap
- `crates/server/src/config.rs` — 新增 `MobileConfig`
- `crates/server/src/service/notification.rs` — 新增 APNs 渠道
- `crates/server/src/service/auth.rs` — validate_session 签名变更 + source-aware TTL + 测试更新
- `crates/server/Cargo.toml` — 新增 `a2` 依赖

### iOS (Swift)

- `apps/ios/ServerBee/Views/Auth/QRScannerView.swift`
- `apps/ios/ServerBee/Services/PushNotificationManager.swift`
- `apps/ios/ServerBee/ServerBee.entitlements` — Push Notification entitlement

### 修改文件 (Swift)

- `apps/ios/ServerBee/Views/Auth/LoginView.swift` — 添加扫码入口
- `apps/ios/ServerBee/ServerBeeApp.swift` — APNs 注册 + AppDelegate
- `apps/ios/ServerBee/ContentView.swift` — deep link 导航
- `apps/ios/ServerBee/Services/APIClient.swift` — 统一解包 ApiResponse
- `apps/ios/ServerBee/ViewModels/ServersViewModel.swift` — 去掉 ApiResponse 手动解包
- `apps/ios/ServerBee/ViewModels/ServerDetailViewModel.swift` — 同上
- `apps/ios/ServerBee/ViewModels/AlertsViewModel.swift` — 同上
- `apps/ios/ServerBee/ViewModels/AlertDetailViewModel.swift` — 同上
- `apps/ios/ServerBee/Info.plist` — NSCameraUsageDescription
- `apps/ios/project.yml` — 添加 entitlements 路径 + Push Notifications capability

### Web (React)

- `apps/web/src/routes/_authed/settings/mobile-devices.tsx` — 设备管理页
- `apps/web/src/components/mobile-pair-dialog.tsx` — QR 配对弹窗

### 文档

- `ENV.md` — 新增 SERVERBEE_MOBILE__* 环境变量
- `apps/docs/content/docs/en/configuration.mdx` — 新增 [mobile] 配置段
- `apps/docs/content/docs/cn/configuration.mdx` — 同上中文版
