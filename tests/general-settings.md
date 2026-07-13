# 通用设置页测试用例

## 前置条件

参照 [README.md](README.md) 启动 Server 并以管理员登录。Agent onboarding 不在 Settings 页进行，入口是 **Add Server**。

## 一、页面加载与数据源

| # | 测试场景 | 操作步骤 | 预期结果 |
|---|---------|---------|---------|
| GS-1 | 页面正常加载 | 导航到 `/settings` | 显示 Settings 标题，页面无报错 |
| GS-2 | GeoIP 行 | 查看 Data Sources | 展示安装状态和 Download/Update 操作，并显示 DB-IP attribution |
| GS-3 | ASN 行 | 查看 Data Sources | 展示 ASN 数据源状态与可用操作 |
| GS-4 | About 行 | 查看 About | 显示当前 ServerBee 版本 |
| GS-5 | Member 权限 | 以 member 登录 | 可读取允许的状态，不能执行管理员写操作 |

## 二、API 端点

| # | 测试场景 | 操作步骤 | 预期结果 |
|---|---------|---------|---------|
| API-1 | 获取系统设置 | `GET /api/settings` | 200，返回站点设置，不包含 Agent 密钥 |
| API-2 | 更新系统设置 | `PUT /api/settings` with `{"site_name":"Test"}` | Admin 返回 200；member 返回 403 |
| API-3 | 数据库备份 | `POST /api/settings/backup` | 200，响应为 SQLite 备份附件 |
| API-4 | 恢复无效文件 | `POST /api/settings/restore` with 非 SQLite 数据 | 422 Unprocessable Entity |
| API-5 | 未认证访问 | 不带凭据调用 `GET /api/settings` | 401 |

## 三、Agent lifecycle 边界

| # | 测试场景 | 操作步骤 | 预期结果 |
|---|---------|---------|---------|
| B-1 | Settings 不承载全局注册码 | 检查 `/settings` | 不显示全局 Agent key、全局 offer 列表或生成入口 |
| B-2 | Add Server 是 onboarding 入口 | 点击侧栏 **Add Server** | Server 配置和绑定 offer 一次创建，明文 code 只显示一次 |

## 四、i18n

| # | 测试场景 | 操作步骤 | 预期结果 |
|---|---------|---------|---------|
| I18N-1 | 英文模式 | 英文下查看 | Settings、Data Sources、About 文案正确 |
| I18N-2 | 中文模式 | 切换中文 | 设置、数据源、关于文案正确，无缺失 key |
