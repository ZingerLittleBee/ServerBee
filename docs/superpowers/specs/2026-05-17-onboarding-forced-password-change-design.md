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
  - **路径匹配口径**（修正自 review P2）：API router 经 `.nest("/api", api::router(...))` 挂载（`crates/server/src/router/mod.rs:21`），auth_middleware 在 nest 内运行时 `req.uri().path()` 已被 strip 成 `/auth/me`（不含 `/api`）。白名单匹配以 **stripped 路径**为准（`/auth/me`、`/auth/onboarding`、`/auth/logout`），不要写 `/api/...` 前缀；如需原始路径用 `axum::extract::OriginalUri`。集成测试用真实完整路径 `/api/auth/onboarding` 打，验证 strip 后匹配正确。
  - 其余 HTTP 请求返回 `403`。**错误码约定**（修正自 review P1）：现有 `AppError::Forbidden(s)` 经 `IntoResponse` 产出 `{ error: { code: "FORBIDDEN", message: "Forbidden: {s}" } }`（见 `crates/server/src/error.rs:39,69`），`code` 对所有 Forbidden 都是 `FORBIDDEN`，无法区分。因此中间件**不走 `AppError`**，直接构造自定义 `Response`，body 为 `{ error: { code: "MUST_CHANGE_PASSWORD", message: "Password change required before continuing" } }`，HTTP 403。前端按 `error.code === "MUST_CHANGE_PASSWORD"` 匹配，不依赖 message 文本。
  - `CurrentUser` 需携带 `must_change_password`（从 user 记录读取）。
- **用户会话类 WS 拦截**（修正自 review P1/P2）：must_change_password 时拒绝的是**用户认证的 WS**——browser (`crates/server/src/router/ws/browser.rs:49`)、terminal (`crates/server/src/router/ws/terminal.rs:99`)、docker logs (`crates/server/src/router/ws/docker_logs.rs:83`)。这三处各自在 handler 内做 session/API key/bearer 校验，**不经过 auth_middleware**，需在每个 validator 内分别加 must_change_password 检查并拒绝升级。
- **agent WS 不在拦截范围**（修正自 review P1）：`crates/server/src/router/ws/agent.rs:77` 用 server token 认证，与用户/admin onboarding 状态无关，绑定会破坏探针上报语义。保持不变。
- `MeResponse`（`/api/auth/me`）新增字段 `must_change_password: bool`，`#[derive(ToSchema)]` 同步，OpenAPI 注解更新。
- **`LoginResponse` 同步新增 `must_change_password: bool`**（修正自 review P1）：前端 `useAuth` 登录成功后直接把 `/api/auth/login` 响应塞进 `['auth','me']` cache 并立刻跳转（`apps/web/src/hooks/use-auth.ts:25`、`apps/web/src/routes/login.tsx:29`），若只改 `MeResponse`，首登后 guard 看到的是缺字段的 login response，会先渲染受保护页并连 browser WS。`LoginResponse` 带字段后 guard 立即可判定。
- **mobile 登录拦截**（修正自 review P2）：`/api/mobile/auth/login` 在 `public_router`（`crates/server/src/router/api/mobile.rs:69`），绕过 auth_middleware。`MobileAuthService::login` 在密码校验通过后，若该用户 `must_change_password == true`，返回 `403` 且 body `code = "MUST_CHANGE_PASSWORD"`，**不签发移动设备 token**（避免绕过强制改密拿到长期会话）。`mobile_refresh` / `mobile_pair_redeem` 不受影响（它们针对已存在的设备会话，正常路径下 admin 已完成 onboarding）。

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
  - `new_username`（修正自 review P3）：服务层先 `trim()`；trim 后为空字符串视为"未提供"（不改用户名，不报错）；非空且与现用户名不同时查重，冲突 `409`。
- 行为（`AuthService::complete_onboarding`）：
  - argon2 哈希新密码（复用 `hash_password`）。
  - 更新 `password_hash`，若提供 `new_username` 则更新 `username`，置 `must_change_password = false`，刷新 `updated_at`。
  - 写审计日志 `action = "onboarding"`（best-effort）。
- 响应：`Json<ApiResponse<&'static str>>`，`"ok"`。
- 现有 `PUT /api/auth/password`（`change_password`）保持不变，不改动。
- `#[utoipa::path]` 注解齐全，Swagger 可见。

### 5. 前端

- `apps/web/src/hooks/use-auth.ts`：`MeResponse` 类型经 `api-schema` 自动带出 `must_change_password`，无需手写。
- `_authed` 守卫（`apps/web/src/routes/_authed.tsx`）：
  - 已认证且 `user.must_change_password === true` 时，强制 `navigate({ to: '/onboarding' })`，并阻止渲染常规受保护内容。
  - **WS hook 门控**（修正自 review P2）：当前 `shouldConnectWs = isAuthenticated && !isLoading`（`apps/web/src/routes/_authed.tsx:130`），即使带了字段也会尝试连 browser WS。改为 `isAuthenticated && !isLoading && user?.must_change_password !== true`，确保 must-change 状态下 `useServersWs` 不启动。
- 新路由 `apps/web/src/routes/onboarding.tsx`：
  - 独立布局，不复用 `_authed` 的侧边栏/导航（避免可点击逃逸）。
  - **自身 auth 状态处理**（修正自 review P3，因不在 `_authed` 下）：未认证 → `navigate('/login')`；已认证但 `must_change_password !== true` → `navigate('/')`；加载中显示 loading。不依赖后端 403 兜底来决定页面可用性。
  - 表单字段：
    - 新密码（必填，password input）
    - 确认新密码（必填，前端校验一致）
    - 新用户名（可选，默认值预填**当前 `user.username`**，非硬编码 `admin`；首启路径下它本就是 `admin`；修正自 review P3）
  - 提交调用 `POST /api/auth/onboarding`；成功后 `queryClient.invalidateQueries(['auth','me'])` 并 `navigate({ to: '/' })`。
  - 失败 toast 显示后端错误（重名 / 新旧密码相同等）。
- **api-client 兜底跳转方式**（修正自 review P2）：`apps/web/src/lib/api-client.ts` 是纯 fetch wrapper（`apps/web/src/lib/api-client.ts:11`），无 React/router 上下文，不能在其中调用 hook 或 `navigate`。解析响应体拿到 `error.code === "MUST_CHANGE_PASSWORD"` 时，用 `window.location.assign('/onboarding')` 做硬兜底（仅作为 guard 未覆盖边缘路径的保险，正常流程由 `_authed` guard 处理）。`ApiError` 增加可选 `code` 字段以承载后端 error code。

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
- must_change_password 会话访问任意受保护路由（如 `GET /api/servers`）返回 `403` 且 body `error.code == "MUST_CHANGE_PASSWORD"`。
- 白名单路由（`/api/auth/me`、`/api/auth/onboarding`、`/api/auth/logout`）在该状态下可访问。
- `/api/auth/login` 与 `/api/auth/me` 响应均含 `must_change_password: true`。
- onboarding 成功后，同会话再访问受保护路由恢复 `200`。
- onboarding 改用户名后，用新用户名 + 新密码可重新登录。
- mobile：`/api/mobile/auth/login` 对 must_change_password 用户返回 `403`（code=MUST_CHANGE_PASSWORD），且不创建移动设备会话；onboarding 后再 mobile login 成功。
- 用户会话类 WS：must_change_password 时 browser / terminal / docker logs WS 升级被拒；agent WS（server token）不受影响仍可连接。

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
- `crates/server/src/migration/m20260517_000023_add_must_change_password.rs` — 新 migration（接续现有全局递增序列，最新为 `m20260517_000022_create_agent_enrollment.rs`；修正自 review P3）
- `crates/server/src/migration/mod.rs` — 注册 migration
- `crates/server/src/middleware/auth.rs` — 硬拦截（自定义 403 Response，code=MUST_CHANGE_PASSWORD）+ CurrentUser 带字段
- `crates/server/src/error.rs` — 参考（确认 Forbidden code 不可区分，故中间件走自定义 Response）
- `crates/server/src/router/api/auth.rs` — onboarding handler + MeResponse 字段 + **LoginResponse 字段** + 路由注册
- `crates/server/src/router/ws/browser.rs` — validator 内加 must_change_password 拒绝
- `crates/server/src/router/ws/terminal.rs` — validator 内加 must_change_password 拒绝
- `crates/server/src/router/ws/docker_logs.rs` — validator 内加 must_change_password 拒绝
- `crates/server/src/service/mobile_auth.rs`（`MobileAuthService::login`）+ `crates/server/src/router/api/mobile.rs` 调用点 — mobile login 对 must_change_password 用户返回 403，不签发设备 token（文件名修正自 review P3）
- `crates/server/src/main.rs` — init_admin 调用点 + banner 增强
- `apps/web/src/routes/onboarding.tsx` — 新页面
- `apps/web/src/routes/_authed.tsx` — 守卫 + WS hook 门控
- `crates/server/src/openapi.rs` — 手动注册 onboarding path / OnboardingRequest schema（修正自 review P2）
- `apps/web/src/lib/api-client.ts` — ApiError 加 code 字段 + window.location.assign 兜底
- `apps/web/src/hooks/use-auth.ts` / `apps/web/src/routes/login.tsx` — 参考（LoginResponse 带字段后 guard 即时生效，确认无需强制 refetch）
- `ENV.md` / `apps/docs/content/docs/{en,cn}/configuration.mdx` / README — 删 env 引导
