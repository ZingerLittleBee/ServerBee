# GeoIP 数据库管理测试用例

## 前置条件

参照 [TESTING.md](../TESTING.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

---

## 一、Settings 页面（/settings/geoip）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| GEO1 | 未安装状态 | 导航到 Settings → GeoIP | 显示 "Not Installed" + Download 按钮 + DB-IP CC BY 4.0 归属 | — |
| GEO2 | 下载 GeoIP | 点击 Download | loading 状态 → 成功 toast → 状态切换为 "Installed" + 文件大小 + 更新日期 | — |
| GEO3 | 更新 GeoIP | 已安装时点击 "Update" | RefreshCw 图标 → 可重新下载最新版 | — |
| GEO4 | 自定义 MMDB | 配置 geoip.mmdb_path | 显示 "Using custom MMDB file"，无 Download/Update 按钮 | — |

---

## 二、Server Map Widget 集成

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| GEO5 | 下载提示 | 未安装 GeoIP → Server Map widget | 显示 "GeoIP database not installed" + Download 按钮 | — |
| GEO6 | member 用户 | member 用户查看 Server Map | 看到未安装提示但无 Download 按钮 | — |

---

## 三、导航与权限

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| GEO7 | 侧边栏导航 | admin 用户查看侧边栏 | Settings 下显示 GeoIP 菜单项（MapPin 图标），member 不可见 | — |

---

## 四、运行时行为

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| GEO8 | 热加载 | 下载完成后无需重启服务 | Agent 下次上报 SystemInfo 时自动查询 country_code | — |
| GEO9 | 并发下载保护 | 快速双击 Download | 第二次返回 "Download already in progress" | — |
| GEO10 | 脏数据清理 | Agent IP 变为私网 | GeoIP 查询返回 None → country_code 被清除，地图不再显示旧位置 | — |
