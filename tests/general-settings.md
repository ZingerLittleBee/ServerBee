# 通用设置页测试用例

## 前置条件

参照 [README.md](README.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

---

## 一、页面加载与渲染（/settings）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| GS-1 | 页面正常加载 | 登录后导航到 `/settings` | 页面加载完成，显示标题 "Settings" | ✅ |
| GS-2 | 侧边栏导航 | 点击侧边栏 "Settings" 链接 | 导航到 `/settings` | ✅ |
| GS-3 | 两个 Card 区域 | 查看页面 | 显示 Auto-Discovery Key 卡片 + GeoIP 卡片 | ✅ |

---

## 二、Auto-Discovery Key

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| GS-4 | Key 掩码显示 | 加载后查看 key 区域 | Key 默认以 `*` 掩码显示 | ✅ 显示 43 个 `*` |
| GS-5 | 显示 Key | 点击眼睛图标（"Show key"） | Key 明文显示，按钮变为 "Hide key" | ✅ |
| GS-6 | 隐藏 Key | 再次点击（"Hide key"） | Key 恢复掩码，按钮变为 "Show key" | ✅ |
| GS-7 | 复制 Key | 点击复制按钮 | toast 显示 "Copied to clipboard" | ⏭️ headless clipboard 受限 |
| GS-8 | Key 格式 | 显示 Key 后查看 | Key 为非空字符串（43 字符），等宽字体显示 | ✅ |

---

## 三、GeoIP 卡片

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| GS-9 | GeoIP 状态显示 | 查看 GeoIP 卡片 | 显示 "Not Installed" + Download 按钮 + DB-IP 归属 | ✅ |

---

## 四、API 端点验证

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| API-1 | 获取 discovery key | `GET /api/settings/auto-discovery-key` | 200，返回非空 key（43 字符） | ✅ |
| API-2 | 获取系统设置 | `GET /api/settings` | 200，返回 `{site_name, site_description, custom_css, custom_js}` | ✅ |
| API-3 | 更新系统设置 | `PUT /api/settings` with `{"site_name":"Test"}` | 200，返回 `{site_name:"Test",...}` | ✅ |
| API-4 | 重新生成 key | `PUT /api/settings/auto-discovery-key` | 200，返回新 key（与原 key 不同） | ✅ old≠new 验证通过 |
| API-5 | 数据库备份 | `POST /api/settings/backup` | 200，Content-Disposition: `attachment; filename="serverbee_backup_*.db"` | ✅ |
| API-6 | 恢复无效文件 | `POST /api/settings/restore` with 非 SQLite 数据 | 422 Unprocessable Entity | ✅ |
| API-7 | 恢复过小文件 | `POST /api/settings/restore` with 小于 16 字节 | 422 Unprocessable Entity | ✅ |
| API-8 | 未认证访问 | 不带 cookie → `GET /api/settings` | 401 | ✅ |

---

## 五、i18n

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| I18N-1 | 英文模式 | 英文下查看 | "Settings"、"Auto-Discovery Key" 英文 | ✅ |
| I18N-2 | 中文模式 | 切换中文 | 标题显示 "设置" | ✅ |

---

## 测试统计

| 模块 | 用例数 | ✅ | ⏭️ | — |
|------|--------|-----|------|-----|
| 页面加载与渲染 | 3 | 3 | 0 | 0 |
| Auto-Discovery Key | 5 | 4 | 1 | 0 |
| GeoIP 卡片 | 1 | 1 | 0 | 0 |
| API 端点验证 | 8 | 8 | 0 | 0 |
| i18n | 2 | 2 | 0 | 0 |
| **合计** | **19** | **18** | **1** | **0** |

- ✅ 通过：18 (94.7%)
- ⏭️ 跳过（clipboard API 在 headless 环境受限）：1 (5.3%)
