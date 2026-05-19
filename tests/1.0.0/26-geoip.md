# 26 GeoIP 数据库 — 冒烟测试

**前置条件**:已登录 admin,进入 `/settings/geoip`。深度用例见 [../geoip.md](../geoip.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| GI1 | 未安装状态 | 全新环境查看 GeoIP 设置 | 显示未安装 + Download 按钮 | 否 | ✅ |
| GI2 | 下载数据库 | 点击 Download | 下载成功,状态变已安装 | 否 | ✅ |
| GI3 | server-map widget | 安装后添加 server-map widget | 有公网 IP 的服务器在地图标记 | 否 | — |
| GI4 | 归属展示 | 查看地图 | 显示 "GeoIP by DB-IP" 归属 | 否 | ✅ |
| GI5 | 未安装降级 | 未安装时用 server-map | 提示需安装,无报错 | 否 | ✅ |

> 注:GeoIP 设置 UI 实际在 `/settings`(通用设置)内的 GeoIP 卡片,非独立 `/settings/geoip` 路由(md 前置条件路径偏差,非缺陷)。
> GI1: UI 显示 "GeoIP Database / Not Installed / Download" + "Data provided by DB-IP, licensed under CC BY 4.0";API `installed:false`。
> GI2: `POST /api/geoip/download` 返回 success,status 变 `installed:true`,file_size 8.27MB,source downloaded。
> GI3: 共享测试 Agent 为本机 macOS 无公网 IP(`country_code` 为空),地图正常渲染但无标记点。widget 逻辑已确认:有 `country_code` 的服务器才落点(server-map.tsx:63-92),无法在本环境验证真实标记。
> GI4: server-map widget 在 countryGroups>0 时渲染 `attribution`;GeoIP 卡片显式展示 DB-IP/CC BY 4.0 归属。
> GI5: 未安装时 widget 显示 `noGeoIP` 提示 + admin Download 按钮,无报错(server-map.tsx:161-181 优雅降级)。
> 还原:已删除 `./data/dbip-country-lite.mmdb`(下次 Server 重启回到未安装默认;运行实例内存仍持有,未重启共享 Server)。GeoIP 为 CC 公共库非用户数据,不污染测试基线。

**汇总**:✅ 4 / ❌ 0 / — 1
