# 首次启动随机密码 + 强制 Onboarding 改密 — 设计文档

- 日期: 2026-05-17
- 状态: 已确认，待实现计划
- 范围: server (Rust) + web (React) + 文档

## 背景与动机

当前 admin 凭证通过环境变量 `SERVERBEE_ADMIN__USERNAME` / `SERVERBEE_ADMIN__PASSWORD` 配置（Figment 加载）。明文密码出现在 docker-compose、环境变量、`docker inspect` 中，属于不良实践。

`AuthService::init_admin` 已具备"密码为空则随机生成并打印 banner"的能力，但：

1. 仍保留"配了环境变量就用配的"分支，引导用户走 env 配置。
2. 没有"首次登录强制改密"机制——随机密码长期有效。

参考 Gitea（`--random-password` + `--must-change-password` 默认 true）、Portainer（首次向导）的成熟范式，本设计将 ServerBee 收敛为：**首次启动必随机生成密码 → 打印高可见日志 banner → 用户首次登录被强制走 onboarding 改密（可选顺带改用户名）**。

## 决策记录

| 决策点 | 选择 | 理由 |
|---|---|---|
| username 是否可配置 | 否。删除 `AdminConfig`，硬编码默认 `admin`，onboarding 页可选改名 | 主流项目用户名均默认 admin 且非必填环境变量；多一个配置项零收益 |
| 后端约束强度 | 后端硬拦截（纵深防御） | 与项目 capability 双端校验风格一致，不可绕过 |
| 存量部署兼容 | 存量不受影响 | 新列 DEFAULT false，仅新建首个 admin 标记 true，升级无感 |
| onboarding 接口 | 新增独立 `POST /api/auth/onboarding`，不复用 `PUT /api/auth/password` | 语义不同：onboarding 无 old_password、支持改用户名 |

## 详细设计

### 1. 配置层

- 删除 `crates/server/src/config.rs` 中的 `AdminConfig` 结构、`AppConfig.admin` 字段、`default_admin_username` 及相关 serde 默认。
- `AuthService::init_admin` 签名改为不再接收 `admin_config`：
  - 用户名硬编码常量 `DEFAULT_ADMIN_USERNAME = "admin"`。
  - 密码**始终**随机生成（移除"配了就用配的"分支），复用现有 `generate_session_token()`。
  - `users` 表非空时仍直接返回（跳过初始化），逻辑不变。
- 文档同步删除 `SERVERBEE_ADMIN__*` 引导：
  - `ENV.md`
  - `apps/docs/content/docs/{en,cn}/configuration.mdx`
  - 根目录 README / docker-compose 示例（移除 `SERVERBEE_ADMIN__USERNAME` / `SERVERBEE_ADMIN__PASSWORD` environment 行）

### 2. 数据模型

- 新增 migration（`crates/server/src/migration/`，仅实现 `up()`，`down()` 留空 `Ok(())`）：
  - `users` 表新增列 `must_change_password BOOLEAN NOT NULL DEFAULT 0`。
- `crates/server/src/entity/user.rs`：`Model` 增加字段 `pub must_change_password: bool`。
- 存量用户走 DEFAULT 0；`init_admin` 创建首个 admin 的 `ActiveModel` 显式 `must_change_password: Set(true)`。

### 3. 后端硬拦截

- `crates/server/src/middleware/auth.rs`：认证解析出 `CurrentUser` 后，新增检查——
  - 若 `must_change_password == true`，仅放行白名单：
    - `GET /api/auth/me`
    - `POST /api/auth/onboarding`
    - `POST /api/auth/logout`
  - 其余 HTTP 请求返回 `403`：复用现有 `AppError::Forbidden`，message 固定为机器可读标识字符串 `must_change_password`（前端按此字符串匹配做兜底跳转）。不新增 `AppError` 变体。
  - WebSocket 升级请求同样在 must_change_password 时拒绝（终端 / agent / browser WS 均不可用）。
  - `CurrentUser` 需携带 `must_change_password`（从 user 记录读取）。
- `MeResponse`（`/api/auth/me`）新增字段 `must_change_password: bool`，`#[derive(ToSchema)]` 同步，OpenAPI 注解更新。

### 4. Onboarding 接口

新增 `POST /api/auth/onboarding`：

- 路由：注册在**已认证**路由组（需有效 session），但由步骤 3 的白名单确保 must_change_password 会话能访问。
- 请求体 `OnboardingRequest`（`#[derive(Deserialize, ToSchema)]`）：
  - `new_password: String` — 必填
  - `new_username: Option<String>` — 可选
- 校验：
  - `new_password` 非空。
  - `new_password` 不等于当前密码（用现有 `verify_password` 比对，相等则 `422`）。
  - 仅当当前用户 `must_change_password == true` 时允许调用（否则 `403`，防止误用）。
  - `new_username` 若提供且与现用户名不同：查重，冲突 `409`。
- 行为（`AuthService::complete_onboarding`）：
  - argon2 哈希新密码（复用 `hash_password`）。
  - 更新 `password_hash`，若提供 `new_username` 则更新 `username`，置 `must_change_password = false`，刷新 `updated_at`。
  - 写审计日志 `action = "onboarding"`（best-effort）。
- 响应：`Json<ApiResponse<&'static str>>`，`"ok"`。
- 现有 `PUT /api/auth/password`（`change_password`）保持不变，不改动。
- `#[utoipa::path]` 注解齐全，Swagger 可见。

### 5. 前端

- `apps/web/src/hooks/use-auth.ts`：`MeResponse` 类型经 `api-schema` 自动带出 `must_change_password`，无需手写。
- `_authed` 守卫（`apps/web/src/routes/_authed.tsx`）：已认证且 `user.must_change_password === true` 时，强制 `navigate({ to: '/onboarding' })`，并阻止渲染常规受保护内容。
- 新路由 `apps/web/src/routes/onboarding.tsx`：
  - 独立布局，不复用 `_authed` 的侧边栏/导航（避免可点击逃逸）。
  - 表单字段：
    - 新密码（必填，password input）
    - 确认新密码（必填，前端校验一致）
    - 新用户名（可选，默认值预填 `admin`，placeholder 提示"可改可不改"）
  - 提交调用 `POST /api/auth/onboarding`；成功后 `queryClient.invalidateQueries(['auth','me'])` 并 `navigate({ to: '/' })`。
  - 失败 toast 显示后端错误（重名 / 新旧密码相同等）。
- `apps/web/src/lib/api-client.ts`：收到 `403` 且响应体含 `must_change_password` 标识时，兜底 `navigate('/onboarding')`（防止守卫未覆盖的边缘路径）。

### 6. 启动日志 banner

- 保留现有星号框 banner（`crates/server/src/main.rs`），增强可见性：
  - 改用 `tracing::warn!`（默认配色更显眼），多行框。
  - 文案明确：首次启动专属、该密码仅显示这一次、登录后会被强制修改。
  - username 行固定显示 `admin`。
  - 中英不强制双语，沿用现有英文风格即可。

### 7. 测试

Rust 单测（`crates/server`）：
- `init_admin`：users 表空时必创建 admin，密码随机非空，`must_change_password == true`，用户名为 `admin`；users 表非空时返回 `None` 不创建。
- `complete_onboarding`：成功置 false 并改密；新旧密码相同被拒；空密码被拒；用户名重名被拒；当前用户非 must_change_password 时被拒。

集成测试（`crates/server/tests/integration`）：
- must_change_password 会话访问任意受保护路由（如 `GET /api/servers`）返回 `403` 且体含标识。
- 白名单路由（`/api/auth/me`、`/api/auth/onboarding`、`/api/auth/logout`）在该状态下可访问。
- onboarding 成功后，同会话再访问受保护路由恢复 `200`。
- onboarding 改用户名后，用新用户名 + 新密码可重新登录。

前端 vitest（`apps/web`）：
- 守卫：`me.must_change_password === true` 时跳转 `/onboarding`。
- onboarding 表单：确认密码不一致时阻止提交。

## 非目标 (YAGNI)

- 不引入密码强度策略 / 复杂度校验（保持与现有 `change_password` 一致，仅非空）。
- 不引入 `*_FILE` / Docker secrets 机制。
- 不强制存量用户改密。
- 不做 onboarding 多步向导（单页表单足够）。
- 不改动现有 2FA、OAuth、API key 流程。

## 影响文件清单（预估）

- `crates/server/src/config.rs` — 删 AdminConfig
- `crates/server/src/service/auth.rs` — init_admin 改签名、新增 complete_onboarding
- `crates/server/src/entity/user.rs` — 新增字段
- `crates/server/src/migration/m20260517_000001_add_must_change_password.rs` — 新 migration（按现有 `mYYYYMMDD_NNNNNN_` 命名约定）
- `crates/server/src/migration/mod.rs` — 注册 migration
- `crates/server/src/middleware/auth.rs` — 硬拦截 + CurrentUser 带字段
- `crates/server/src/router/api/auth.rs` — onboarding handler + MeResponse 字段 + 路由注册
- `crates/server/src/main.rs` — init_admin 调用点 + banner 增强
- `apps/web/src/routes/onboarding.tsx` — 新页面
- `apps/web/src/routes/_authed.tsx` — 守卫
- `apps/web/src/lib/api-client.ts` — 403 兜底
- `ENV.md` / `apps/docs/content/docs/{en,cn}/configuration.mdx` / README — 删 env 引导
