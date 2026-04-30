# 自定义主题系统设计

## 1. 背景

P14 已经为前端引入了 8 套基于 OKLCH CSS 变量的预设主题(default + 7 个具名主题),由 `apps/web/src/components/theme-provider.tsx` 通过 `data-theme` 属性 + 异步加载 CSS 文件的方式应用。预设主题选择目前仅持久化到 `localStorage`,未落库;客户端侧无法被分享、同步或多端复用。

随着 P15 状态页(`status_page`)的多租户化,**不同状态页可能服务于不同的业务方**(主站、API、CDN…),它们对外的视觉品牌经常需要差异化。当前架构下,所有页面只能共享同一套 colorTheme,这是产品上的瓶颈。

本设计提出在不破坏现有 8 套预设的前提下,新增"用户可自定义主题"能力,并把后台主题与每个状态页的主题解耦,使各自可独立选择。

## 2. 目标与非目标

### 2.1 目标

- 复用现有 OKLCH CSS 变量体系,不引入新的样式生成层。
- 允许 Admin **创建任意多套**自定义主题(JSON 形式承载变量值),与 8 套预设并列展示。
- 后台管理面板与每个状态页可**独立绑定**主题(预设或自定义)。
- 提供可视化编辑器:左栏分组变量编辑 + 右栏隔离实时预览。
- 主题可导入 / 导出为 JSON,便于跨部署迁移与团队内分享。
- 主题数据落库到服务端,`localStorage` 仅作首屏防闪缓存。

### 2.2 非目标

- 不修改预设主题(8 套)的 CSS 文件结构。
- 不实现外部 URL 拉取主题(避免 SSRF / 减少首版攻击面)。
- 不实现 marketplace / 远程主题市场。
- 不允许自定义主题修改布局或注入任意 CSS,主题作用范围严格限制在 ~25 个白名单 CSS 变量。
- 不实现 WebSocket 推送的实时主题切换,首版依赖客户端轮询/重新挂载。
- 不实现撤销/重做 / 版本历史。

## 3. 现状分析

| 维度 | 现状 |
|---|---|
| 主题载体 | `apps/web/src/themes/*.css`,每个文件提供 `[data-theme="..."]` 与 `[data-theme="..."].dark` 两组规则 |
| 变量集合 | `background / foreground / card / popover / primary / secondary / muted / accent / destructive / border / input / ring / chart-1~5 / sidebar-*` 约 25 个 |
| 选择持久化 | `localStorage.color-theme`,纯前端 |
| 切换机制 | `ThemeProvider` 通过 `loadThemeCSS()` 动态 import + 设置 `data-theme` 根属性 |
| Brand 设置 | 已有服务端表 `brand`(logo / favicon / site_title / footer_text),`/api/settings/brand` |
| 状态页 | P15 多状态页架构,`status_page` 表已有 N 行,公开页面通过 `slug` 路由 |
| RBAC | Admin 与 Member 两档,`require_admin` 中间件已就位 |

## 4. 方案选择

### 4.1 方案 A:轻度自定义(只挑主色)

只让用户挑一个主色,系统按色相算法派生其他变量。

- **优点**:实现极简、用户决策成本低。
- **缺点**:派生出来的暗色 / 边框 / 图表色质量普遍低于人工调色,自由度太弱;无法覆盖"想要某种品牌灰"这类场景。

### 4.2 方案 B:CSS 注入

在外观设置加一个文本框,用户写自定义 CSS 注入到页面。

- **优点**:开发量最小,自由度最大。
- **缺点**:面向写 CSS 的工程师,普通运维不会用;没有预览;一段错误的 CSS 可能整页错位;扩大攻击面(尽管限定 Admin)。

### 4.3 方案 C:精简主题包(本设计选用)

主题 = 一份 JSON,只承载 ~25 个 CSS 变量的 light / dark 两组值;提供可视化编辑器与导入导出。

- **优点**
  - 完全复用既有 OKLCH 变量体系,不破坏 8 套预设。
  - 普通用户用拾取器选色;高级用户导出 JSON 共享。
  - 严格的变量白名单 + 格式校验,可控可审计。
  - 与"多状态页独立主题"是天然兼容的(同一份变量集既能挂全局也能挂 status_page)。
- **缺点**
  - 改不了布局或组件结构,纯视觉。

### 4.4 方案 D:主题包 + CSS 注入混合

C 之上叠加 CSS 注入。

- **优点**:同时覆盖普通用户与极客用户。
- **缺点**:实现成本翻倍,首版价值不足以摊薄。可作 P17+ 拓展。

**结论:采用方案 C。** 后续若需要 D,在主题表上增加一个可空的 `extra_css` 列即可平滑扩展,无需重构。

## 5. 数据模型

### 5.1 新表 `custom_theme`

```text
custom_theme
  id            INTEGER PRIMARY KEY AUTOINCREMENT
  name          TEXT    NOT NULL                -- 用户取的名字,UNIQUE 不强制(允许同名)
  description   TEXT                            -- 可选说明
  based_on      TEXT                            -- fork 自哪个预设的 id,展示用,可空
  vars_light    TEXT    NOT NULL                -- JSON 序列化的变量映射
  vars_dark     TEXT    NOT NULL                -- 同上
  created_by    TEXT    NOT NULL                -- 引用 users.id (String 主键),不建外键以便用户被删后保留审计字段
  created_at    INTEGER NOT NULL
  updated_at    INTEGER NOT NULL
  INDEX idx_custom_theme_updated_at (updated_at DESC)
```

`vars_light` / `vars_dark` 的 JSON 形如:

```json
{
  "background": "oklch(0.96 0.01 290)",
  "foreground": "oklch(0.3 0.03 290)",
  "card": "oklch(0.97 0.008 290)",
  "...": "..."
}
```

### 5.2 全局激活值

复用现有 `configs` KV 表 + `ConfigService`(Brand 已经用同样的方式持久化,key 为 `"brand"`),新增一个 key:

```text
key:   active_admin_theme
value: "preset:default"   -- 或 "preset:tokyo-night" / "custom:42"
```

URN 字符串前缀(`preset:` / `custom:`)用于 dispatch,避免"两字段同空 / 同满"的非法状态。

### 5.3 `status_page` 扩展

```text
ALTER TABLE status_page
  ADD COLUMN theme_ref TEXT NULL;   -- 同样的 URN 风格;NULL = 跟随后台激活主题
```

### 5.4 变量白名单

服务端硬编码的变量 key 集合,导入 / 创建 / 更新时校验:

```text
background, foreground, card, card-foreground, popover, popover-foreground,
primary, primary-foreground, secondary, secondary-foreground,
muted, muted-foreground, accent, accent-foreground,
destructive, border, input, ring,
chart-1, chart-2, chart-3, chart-4, chart-5,
sidebar, sidebar-foreground, sidebar-primary, sidebar-primary-foreground,
sidebar-accent, sidebar-accent-foreground, sidebar-border, sidebar-ring
```

**所有 key 都必须出现**(必填,缺一个返回 422),保证一份主题永远是"完整可应用"的。值必须匹配下列允许 alpha 通道的 OKLCH 正则:

```text
^oklch\(\s*[\d.]+\s+[\d.]+\s+[\d.]+(\s*/\s*[\d.]+%?)?\s*\)$
```

即 `oklch(L C H)` 或 `oklch(L C H / α)`(α 为 `0.0~1.0` 数字或 `0%~100%` 百分号);多余的空白容错。这一允许 alpha 的形式覆盖了当前 `apps/web/src/index.css` 中已存在的 `oklch(1 0 0 / 10%)` / `oklch(1 0 0 / 15%)` 用法,避免"导入自家默认主题反被校验拒绝"的回环。

## 6. API 设计

遵循 `read_router()` / `write_router()` 拆分、`Json<ApiResponse<T>>` 包裹、`utoipa` 标注的项目惯例。

### 6.1 主题 CRUD

| 方法 | 路径 | 权限 | 说明 |
|---|---|---|---|
| GET | `/api/settings/themes` | 已登录 | 列出所有自定义主题(摘要:`id, name, based_on, updated_at`,不含 `vars`) |
| GET | `/api/settings/themes/:id` | 已登录 | 取单个完整主题 |
| POST | `/api/settings/themes` | Admin | 创建。Body: `{ name, description?, based_on?, vars_light, vars_dark }` |
| PUT | `/api/settings/themes/:id` | Admin | 整体更新 |
| DELETE | `/api/settings/themes/:id` | Admin | 删除。被引用时返回 `409 Conflict`,`message` 含简明提示("Theme is in use") |
| GET | `/api/settings/themes/:id/references` | Admin | 查询本主题被谁引用(用于删除前置确认对话框) |
| POST | `/api/settings/themes/:id/duplicate` | Admin | 复制为新主题(`name` 自动追加 `(copy)`) |

### 6.2 激活与绑定

| 方法 | 路径 | 权限 | 说明 |
|---|---|---|---|
| GET | `/api/settings/active-theme` | 已登录 | 返回 **resolved payload**,见下 |
| PUT | `/api/settings/active-theme` | Admin | 切换。Body: `{ ref: "preset:default" \| "custom:42" }`。返回与 GET 同形 |

`GET /api/settings/active-theme` 响应:

```json
{
  "data": {
    "ref": "custom:42",
    "theme": {
      "kind": "custom",
      "id": 42,
      "name": "My Brand",
      "vars_light": { "...": "..." },
      "vars_dark":  { "...": "..." },
      "updated_at": 1714000000
    }
  }
}
```

如果 `ref` 是 `preset:*`,`theme.kind = "preset"` 且 `vars_light / vars_dark` 字段缺省(预设值已经在客户端代码里),只返回 `{ kind: "preset", id: "default" }`。这样客户端凭一次接口即可应用主题,不需要再请求 `GET /themes/:id`,首屏防闪缓存也能完整保存。

状态页绑定走**已有的** `PUT /api/status-pages/:id`,在 body 里追加可选 `theme_ref` 字段,不开新接口。

### 6.3 导入 / 导出

| 方法 | 路径 | 权限 | 说明 |
|---|---|---|---|
| GET | `/api/settings/themes/:id/export` | 已登录 | 返回完整 JSON,前端可下载或粘到剪贴板 |
| POST | `/api/settings/themes/import` | Admin | Body 即 export 的 JSON,严格 schema 校验后落库 |

导出 / 导入 JSON 形态:

```json
{
  "version": 1,
  "name": "My Brand",
  "description": "Internal dashboard theme",
  "based_on": "tokyo-night",
  "vars_light": { "...": "..." },
  "vars_dark":  { "...": "..." }
}
```

`version` 字段为前向兼容预留;首版只接受 `1`,未知版本号返回 `422`。

### 6.4 错误响应

沿用项目现有的 `AppError` → `{ error: { code, message } }` 包络,不引入额外结构化错误体。

- `409 Conflict`(删除被引用的主题):

  ```json
  {
    "error": {
      "code": "CONFLICT",
      "message": "Theme is in use by admin or one or more status pages; unbind it first."
    }
  }
  ```

  前端在用户点 "删除" 前先调 `GET /api/settings/themes/:id/references`,响应是常规 `ApiResponse`:

  ```json
  {
    "data": {
      "admin": true,
      "status_pages": [{ "id": 3, "name": "Public Status" }]
    }
  }
  ```

  UI 据此弹"主题被以下位置使用,请先解绑"对话框,而不是把结构化引用列表硬塞到错误响应里。

- `422 Unprocessable Entity`(变量校验失败):列出第一个不合法的 key 与原因。

### 6.5 OpenAPI

每个端点 `#[utoipa::path]`,DTO `#[derive(ToSchema)]`,在 `/swagger-ui/` 可见。

## 7. 后端实现

### 7.1 entity

`crates/server/src/entity/custom_theme.rs`,沿用 sea-orm DeriveEntityModel + DeriveActiveModel 模板。

### 7.2 service

`crates/server/src/service/custom_theme.rs`,unit struct 静态方法:

```text
CustomThemeService::list(db) -> Vec<ThemeSummary>
CustomThemeService::get(db, id) -> Theme
CustomThemeService::create(db, payload, user_id) -> Theme
CustomThemeService::update(db, id, payload) -> Theme
CustomThemeService::delete(db, id) -> Result<(), AppError>   // 内部检查引用
CustomThemeService::duplicate(db, id) -> Theme
CustomThemeService::import(db, payload, user_id) -> Theme
CustomThemeService::export(db, id) -> ExportPayload
CustomThemeService::active_theme(db) -> ThemeRef
CustomThemeService::set_active_theme(db, ref) -> ThemeRef    // 校验引用有效
```

### 7.3 引用解析与校验

新增 `crates/server/src/service/theme_ref.rs`:

```text
parse_theme_ref(s: &str) -> ThemeRef           // "preset:default" / "custom:42"
validate_theme_ref(db, ref) -> Result<()>      // 预设白名单 + 自定义存在性
list_theme_references(db, ref) -> ReferenceList // 谁在用这个主题
```

### 7.4 变量校验

`crates/server/src/service/theme_validator.rs`:

- 白名单 key 集合(常量)
- OKLCH 字符串正则 `^oklch\(\s*[\d.]+\s+[\d.]+\s+[\d.]+\s*\)$`(留空格容错)
- 缺 key / 多 key / 格式错误 → `AppError::Validation`

### 7.5 router

`crates/server/src/router/api/theme.rs`:`read_router()` + `write_router()` 拆分,`write_router()` 套 `require_admin` 中间件。

### 7.6 状态页扩展

- `entity/status_page.rs` 加 `theme_ref: Option<String>`
- `service/status_page.rs::update` 接受新字段,写库前调用 `validate_theme_ref`
- 已有公开接口 `GET /api/status/{slug}`(在 `router/api/status_page.rs::public_router()`)的 `PublicStatusPageData` 响应体加一个 `theme: ThemeResolved` 字段:

  ```json
  {
    "theme": {
      "kind": "custom",
      "vars_light": { "..." },
      "vars_dark":  { "..." }
    }
  }
  ```

  这样公开页面无需另发一次请求即可应用主题。

### 7.7 迁移

沿用项目命名 `mYYYYMMDD_NNNNNN_<topic>.rs`,当前最新是 `m20260416_000018_*`,本次新增两个连号文件:

| 文件 | 内容 |
|---|---|
| `m20260430_000019_create_custom_theme.rs` | 建表 `custom_theme` + 索引 `idx_custom_theme_updated_at` |
| `m20260430_000020_add_status_page_theme_ref.rs` | `ALTER TABLE status_page ADD COLUMN theme_ref TEXT NULL` |

无需 seed 迁移:`active_admin_theme` 在 Service 层用 `ConfigService::get(db, "active_admin_theme").await?.unwrap_or_else(|| "preset:default".into())` 按需取默认值,首次写入由 `PUT /api/settings/active-theme` 触发。`ConfigService` 当前仅暴露 `get / set / get_typed / set_typed`,不引入新 helper。

均**只实现 `up()`**,`down()` 留 `Ok(())`。

## 8. 前端实现

### 8.1 路由结构

```text
_authed/settings/appearance.tsx                改造为「主题选择 + 我的主题」页
_authed/settings/appearance/themes.new.tsx     新主题创建(空白或 fork 预设)
_authed/settings/appearance/themes.$id.tsx     编辑器
```

`appearance.tsx` 重构成两栏:**预设区**(8 个固定卡片) + **我的主题区**(自定义列表 + "+ 新建" 按钮)。每张自定义卡片悬停时浮出"编辑 / 复制 / 删除"三个按钮;卡片整体点击 = 激活。

### 8.2 编辑器布局

```text
┌─────────────────────────────┬───────────────────────┐
│ 顶部:[名字输入] [取消] [保存] │                       │
├─────────────────────────────┤                       │
│ [Light] [Dark] tab          │  样例片段:             │
│                             │   - Card + 标题          │
│ 分组手风琴:                  │   - Primary Button       │
│  ▸ 表面色 (background…)     │   - Input + Select       │
│  ▸ 文字色 (foreground…)     │   - Badge × 3            │
│  ▸ 主题色 (primary…)        │   - 折线图 (2 条线)      │
│  ▸ 状态色 (destructive…)    │   - Sidebar 缩略         │
│  ▸ 图表色 (chart-1~5)       │  ─────────────────       │
│  ▸ 边框/输入 (border…)      │  [Light] [Dark] 切换     │
│  ▸ 侧边栏 (sidebar-*)       │  [☐ 与左栏联动]          │
│                             │                          │
│ 每个变量行:                  │                          │
│  [色块] 变量名               │                          │
│  [OKLCH 三轴滑块] [hex 输入] │                          │
│  [↺ 重置为 fork 值]          │                          │
└─────────────────────────────┴───────────────────────┘
```

### 8.3 关键交互

- **拾取器**:OKLCH 三轴滑块(L / C / H)主输入,辅以 hex 双向同步。
- **色彩转换**:`hex ↔ oklch`(及对 alpha 的处理)通过引入 `culori` 依赖完成 —— 它是社区维护良好的 OKLCH/CIE 色彩库,体积小、tree-shakable、覆盖 alpha 通道与 sRGB gamut 落界。**不自实现色彩数学**,色彩空间转换出错难以察觉、调试代价高。如果项目策略不允许新增依赖,退化为只支持 OKLCH 文本输入(无 hex 输入框),不做近似转换。
- **预览隔离**:右栏挂在 `<div data-theme-preview>` 节点,变量通过 `style={{ '--background': ... }}` 注入到该节点 inline,**不影响外层应用**。
- **Light/Dark 联动**:左右 tab 默认联动;勾选"与左栏联动 = off"时右栏可独立切换,便于对比。
- **`isDirty` 拦截**:离开路由时弹确认。
- **fork 来源**:顶部展示 `Based on: <preset name>`,提供"高亮变更"开关(改动过的变量行加灰底)。

### 8.3.1 预设变量来源

当前 8 个预设的真实变量值只存在于 `apps/web/src/themes/*.css` 与 `apps/web/src/index.css`,`apps/web/src/themes/index.ts` 只持有 `previewColors`(4 色卡片预览),fork / 重置 / 比对都需要拿到完整变量 map。

**选用方案:** 在 `apps/web/src/themes/` 新增一个手写的 `preset-vars.ts`(导出 `presetVars: Record<PresetThemeId, { light: VarMap; dark: VarMap }>`),把 8 个预设的全部变量值落到 TS 源码中。

为保证 TS 表与 CSS 文件不漂移,新增一个 vitest 用例 `preset-vars.test.ts`:运行时读取每个 CSS 文件文本,用一个最小化的 CSS 变量 parser 抽取规则,逐 key 与 `presetVars` 对比;不一致即测试失败,提示开发者同步双方。该测试是**唯一的真值同步保险**。

后续若要做"用脚本从 CSS 自动生成 preset-vars.ts",可在 P17+ 再加,首版让作者一次性人工对齐(8 个预设 × 25 变量 × 2 模式,总量可控)。

### 8.4 ThemeProvider 重构

```text
type ColorThemeRef =
  | { kind: 'preset'; id: PresetThemeId }
  | { kind: 'custom'; id: number; vars: { light: VarMap; dark: VarMap } }
```

启动顺序:

1. 同步读 `localStorage.active-theme-ref` → 立刻应用,避免首屏白屏闪烁。
2. 异步 `GET /api/settings/active-theme` → 服务端为准 → 写回 localStorage。

应用方式:

- `kind: 'preset'`:沿用 `loadThemeCSS()` + `data-theme` 属性。
- `kind: 'custom'`:写入 `<style data-theme-runtime>` 标签,内容形如 `:root { --background: ...; } .dark { --background: ...; }`,**不写 `data-theme` 属性**。
- 切换时严格互斥:切到自定义清掉 `data-theme`;切回预设清掉 runtime style。

### 8.5 状态页渲染

公开状态页路由(`/status/:slug` 等)在拿到接口附带的 `theme` 字段后,把变量注入到状态页根节点的 scoped class(如 `<div class="status-page-root">`),不污染外层(状态页内若嵌入了 `dashboard preview widget` 等其他部件)。优先级高于全局后台主题。

### 8.6 状态页编辑表单

`_authed/status-pages/$id/edit.tsx`(或现有等价路径)新增"外观"区块,字段为下拉选择,选项 = "跟随后台主题(默认)" + 8 个预设 + 自定义主题列表。

### 8.7 i18n

新增翻译命名空间:

- `appearance.custom_themes.*` — 列表区文案
- `appearance.editor.*` — 编辑器
- `appearance.editor.groups.*` — 分组名
- `status_page.theme.*` — 状态页主题字段

CN + EN 双语补齐。

### 8.8 localStorage 迁移

P14 时代,`localStorage.color-theme` 是**每个浏览器自己的偏好**(连同 `theme = light/dark/system` 一起),不是全局设置。新版本里"全局激活主题"由 Admin 在服务端唯一决定,语义不同 —— 不能把任意客户端的 localStorage 旧值悄悄上推到服务端,因为:

- Member 调 `PUT /api/settings/active-theme` 会被 403 拒绝;
- 任何成员浏览器的旧偏好都可能覆盖 Admin 在服务端选过的值,造成"我同事打开一次浏览器,后台主题就被改了"。

**采用的策略:**

1. 客户端启动时,读 `localStorage.color-theme`(若存在)只用作**首屏防闪的本地兜底**,与新键 `localStorage.active-theme-ref` 并行;然后调 `GET /api/settings/active-theme`,以服务端为准并写入新键 `active-theme-ref`,**不向服务端写**。
2. 若服务端 `active_admin_theme` 仍为缺省值 `preset:default`,且当前用户是 **Admin**,且 Admin **首次进入** `/settings/appearance` 页面时,弹出一次性提示卡片:"检测到你浏览器之前选过 *Tokyo Night* 主题,要不要把它设为后台默认?"用户点"应用"才发起 `PUT /api/settings/active-theme`;点"忽略"则清掉旧 key,以后不再提示。Member 进入该页面不出现该提示。
3. 提示状态本身写在 `localStorage.theme-migration-prompted = "1"` 防止重复弹出。
4. 旧 `localStorage.color-theme` 在用户做出明确选择(应用 / 忽略)后清除;之前一直保留,不会丢失意图。

这条迁移仅一次性,后续版本可移除该提示逻辑。

## 9. 测试策略

### 9.1 后端

| 类型 | 用例 |
|---|---|
| Service 单测 | `create / update / delete / duplicate / import / export / set_active_theme / parse_theme_ref / validate_theme_ref` 全路径 |
| 校验单测 | 缺 key / 多 key / OKLCH 格式错误 / 未知 version |
| 引用检测 | 删除被后台激活 / 被状态页绑定 → 409;解绑后删除成功 |
| 集成测试 `tests/integration/custom_theme.rs` | 登录 admin → POST 创建 → GET 列表 → PUT 更新 → 切换激活 → 状态页绑定 → 删除冲突 → 解绑后删除 |
| 权限 | Member 调写接口 403;未登录 401 |

### 9.2 前端

| 模块 | 用例 |
|---|---|
| `theme-provider.test.tsx` | URN 解析;preset ↔ custom 切换的 DOM 副作用;localStorage 缓存命中 / 失效;迁移分支 |
| `oklch.test.ts` | hex ↔ oklch 双向转换边界值 |
| 编辑器组件 | 变量改动 → 预览节点 inline 变量更新;Light/Dark 联动;`isDirty` 离开拦截 |
| 列表页 | 预设 / 自定义两区渲染;激活高亮;删除冲突 toast |

### 9.3 E2E 手工清单

放在 `tests/appearance/custom-theme.md`:

- 创建自定义主题 → 激活 → 刷新持久化
- 状态页绑定主题 → 公开页面渲染独立配色
- 多用户:Admin 切主题,Member 浏览器刷新 / 30s 后取到新值
- 移动端编辑器布局降级(双栏堆叠为上下)

## 10. 迁移与回滚

### 10.1 数据库

迁移仅 `up()`(项目惯例);新增表 / 列对旧版本透明,二进制回滚不会影响旧版本启动。

### 10.2 客户端

P14 用户首次升级后,Admin 进入外观页时一次性提示导入 `localStorage.color-theme` 旧偏好(详见 8.8);Member 浏览器仅以服务端激活值为准,不写服务端。

### 10.3 回滚路径

- 后端二进制回滚到 P14:旧代码不读 `status_page.theme_ref` 列、不读 `configs("active_admin_theme")`,系统回到"全局只看 localStorage"行为;自定义主题数据在数据库里沉睡,不丢失。
- 前端单独回滚:同上。
- **紧急关闭开关**:Figment 配置 `feature.custom_themes` 默认 `true`;设为 `false` 时:
  1. 后端隐藏 `/api/settings/themes/*` 路由(返回 404),前端检测到入口接口 404 即自动隐藏"我的主题"区块,只剩 8 个预设。
  2. 后端 `GET /api/settings/active-theme` 在解析 `configs("active_admin_theme")` 时,如果 ref 形如 `custom:*`,**不返回错误**,而是 coerce 成 `preset:default` 后再 resolve,确保前台不会因 feature flag 关闭而白屏。注意:这是读时降级,**不写回 `configs`**,这样下次 flag 重新打开时,原 `custom:*` 激活值仍然恢复有效。
  3. 状态页响应同理:`status_page.theme_ref` 形如 `custom:*` 时被读时 coerce 成 `null`(= 跟随后台),不写回数据库。
  4. `PUT /api/settings/active-theme` 与 `PUT /api/status-pages/:id` 的 `theme_ref` 字段在 flag 关闭时拒绝 `custom:*`(返回 422 "feature disabled"),仅允许 `preset:*` / `null`。

  这套语义保证 flag 切换是**真正可逆的开关**,而不是"关了之后用户的自定义激活值就丢失"。

## 11. 实现里程碑

| 里程碑 | 范围 | 估算 |
|---|---|---|
| **M1 · 数据层** | 迁移 + entity + Service(CRUD / parse_theme_ref / 变量校验) + 单测 | ~1 天 |
| **M2 · API 层** | router / DTO / utoipa / 集成测试 / 权限中间件 | ~1 天 |
| **M3 · ThemeProvider 重构** | URN 解析 + runtime style 注入 + localStorage 迁移 + vitest | ~0.5 天 |
| **M4 · 列表页改造** | `appearance.tsx` 双区 + 编辑/复制/删除/激活按钮 | ~0.5 天 |
| **M5 · 编辑器** | 双栏布局 + 分组手风琴 + OKLCH 拾取器 + 隔离预览 + 导入导出 | ~1.5 天 |
| **M6 · 状态页绑定** | 状态页表单字段 + 公开页面 scoped 注入 | ~0.5 天 |
| **M7 · 文档 + i18n + 验收** | Fumadocs 双语章节 + `ENV.md` + E2E 清单 | ~0.5 天 |

合计约 **5.5 天**,可拆 7 个 PR;也可分两批合(M1–M3 一批,M4–M7 一批)。

## 12. 安全考量

- 所有写接口要求 Admin。
- 变量值严格 OKLCH 正则匹配,不接受任意字符串(防 CSS 注入到样式表)。
- 变量 key 严格白名单匹配,不接受未知 key(防止通过未列出 token 影响布局)。
- 不支持远程 URL 拉取(防 SSRF)。
- 状态页公开接口返回的主题 JSON 只含变量值,不含其他 PII。
- 删除主题 / 切换激活均写 `audit_log`(若存在该表)。

## 13. 后续可扩展点(非本次范围)

- **CSS 注入扩展**:为高级用户在主题表加可空 `extra_css` 列,在 runtime style 标签后追加注入;需要 CSP 调整。
- **主题市场**:增加 `theme_marketplace` 表与远程同步任务,UI 一键安装。
- **协同与版本**:主题历史快照表,支持回滚到上一稿。
- **WebSocket 实时切换**:`BrowserMessage::ThemeChanged { ref }` 让所有在线客户端无需刷新即切换。
