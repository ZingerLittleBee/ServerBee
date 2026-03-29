# iOS 移动端 & Mobile API 测试用例

## 前置条件

参照 [README.md](README.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

iOS 测试需要 Xcode + 模拟器或真机。API 测试可直接用 curl。

```bash
# 获取 session cookie（用于需要认证的管理接口）
curl -s -c /tmp/sb-cookies.txt -X POST http://localhost:9527/api/auth/login \
  -H 'Content-Type: application/json' -d '{"username":"admin","password":"admin123"}'
```

---

## 一、Mobile Auth API（curl 测试）

### 1.1 登录

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| MA-1 | 正常登录 | `curl -X POST http://localhost:9527/api/mobile/auth/login -H 'Content-Type: application/json' -d '{"username":"admin","password":"admin123","installation_id":"test-device-001","device_name":"Test Device"}'` | 200, 返回 `access_token`, `refresh_token`, `user` | — |
| MA-2 | 错误密码 | 同上，密码改为 `wrong` | 401 Unauthorized | — |
| MA-3 | 2FA 必填 | 启用 2FA 后登录不带 `totp_code` | 422 `2fa_required` | — |
| MA-4 | 限流 | 连续 20 次错误登录 | 429 Too Many Requests | — |
| MA-5 | device_name 记录 | 登录后查设备列表 | 设备名显示 "Test Device" | — |

### 1.2 Token 刷新

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| MA-6 | 正常刷新 | `curl -X POST .../api/mobile/auth/refresh -d '{"refresh_token":"<token>","installation_id":"test-device-001"}'` | 200, 返回新的 token 对 | — |
| MA-7 | 旧 token 失效 | 刷新后用旧 access_token 调 `/api/servers` | 401 | — |
| MA-8 | 新 token 有效 | 用新 access_token 调 `/api/servers` | 200 | — |
| MA-9 | 错误 installation_id | refresh 时 installation_id 不匹配 | 401 | — |
| MA-10 | 过期 refresh_token | 等 token 过期（或手动设短 TTL）后 refresh | 401 | — |

### 1.3 Bearer 认证

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| MA-11 | Bearer 访问 API | `curl -H 'Authorization: Bearer <access_token>' http://localhost:9527/api/servers` | 200, 返回服务器列表 | — |
| MA-12 | Bearer 访问 WS | 用 Bearer token 连接 `/api/ws/servers` | WS 连接成功，收到 FullSync | — |
| MA-13 | 无滑动续期 | Bearer token 在 15 分钟后过期（即使一直在用） | 15 分钟后 401 | — |

### 1.4 登出

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| MA-14 | 正常登出 | `curl -X POST .../api/mobile/auth/logout -H 'Authorization: Bearer <token>'` | 200 | — |
| MA-15 | 登出后 token 失效 | 登出后用同一 access_token 调 API | 401 | — |
| MA-16 | 登出清理 device_token | 注册 push token → 登出 → 查 device_tokens 表 | 对应记录已删除 | — |

### 1.5 设备管理

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| MA-17 | 设备列表 | `curl -H 'Authorization: Bearer <token>' .../api/mobile/auth/devices` | 列出当前设备 | — |
| MA-18 | Web 端查看设备 | 用 session cookie 调 `/api/mobile/auth/devices` | 同样可以列出设备 | — |
| MA-19 | 远程注销 | `curl -X DELETE .../api/mobile/auth/devices/<id> -b /tmp/sb-cookies.txt` | 该设备的 session 和 device_token 全部删除 | — |
| MA-20 | 非本人设备 | 尝试注销其他用户的设备 | 404 Not Found | — |

---

## 二、QR 扫码配对

### 2.1 服务端 API

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| QR-1 | 生成配对码 | `curl -X POST .../api/mobile/pair -b /tmp/sb-cookies.txt` | 返回 `{ code: "sb_pair_...", expires_in_secs: 300 }` | — |
| QR-2 | 兑换配对码 | `curl -X POST .../api/mobile/auth/pair -d '{"code":"<code>","installation_id":"qr-device","device_name":"QR Test"}'` | 200, 返回 token 对 | — |
| QR-3 | 一次性使用 | 同一 code 再次兑换 | 400 Invalid pairing code | — |
| QR-4 | 过期码 | 等 5 分钟后兑换 | 401 | — |
| QR-5 | 单用户单码 | 连续生成 2 个码 → 用第 1 个兑换 | 失败（旧码已被清除） | — |

### 2.2 Web 端 UI

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| QR-6 | 导航到页面 | 侧边栏 → 设置 → Mobile Devices | 页面正常加载 | — |
| QR-7 | 添加设备按钮 | 点击 "Add Device" | 弹窗显示 QR 二维码 | — |
| QR-8 | QR 内容正确 | 扫描或解码 QR 内容 | JSON 包含 `type: "serverbee_pair"`, `server_url`, `code` | — |
| QR-9 | 倒计时 | 等待 | 显示剩余时间，5 分钟后显示过期 | — |
| QR-10 | 重新生成 | 过期后点击 Regenerate | 新 QR 码生成 | — |
| QR-11 | 设备列表 | 配对成功后刷新页面 | 新设备出现在列表中 | — |
| QR-12 | 远程注销 | 点击设备的 Revoke 按钮 → 确认 | 设备从列表移除 | — |

### 2.3 iOS 端扫码

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| QR-13 | 扫码按钮 | 登录页点击 "Scan QR Code" | 相机预览打开 | — |
| QR-14 | 相机权限 | 首次打开扫码 | 弹出相机权限请求 | — |
| QR-15 | 扫码成功 | 扫描 Web 端 QR 码 | 自动登录，跳转到主页面 | — |
| QR-16 | 非 ServerBee QR | 扫描随机二维码 | 无反应或提示无效 | — |
| QR-17 | 关闭扫码 | 点击 X 关闭 | 返回登录页 | — |

---

## 三、APNs 推送通知

### 3.1 设备 Token 注册

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| PN-1 | 注册 token | `curl -X POST .../api/mobile/push/register -H 'Authorization: Bearer <token>' -d '{"device_token":"abc123hex"}'` | 200 | — |
| PN-2 | 重复注册 | 同一设备再次注册（不同 token） | 200, token 被更新（upsert by installation_id） | — |
| PN-3 | 注销 token | `curl -X POST .../api/mobile/push/unregister -H 'Authorization: Bearer <token>'` | 200, device_token 行被删除 | — |

### 3.2 Web 端 APNs 渠道配置

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| PN-4 | APNs 类型选项 | 通知渠道创建 → 选择类型 | 下拉菜单包含 "Apple Push Notification (APNs)" | — |
| PN-5 | APNs 表单 | 选择 APNs 类型 | 显示 Key ID、Team ID、Private Key、Bundle ID、Sandbox 字段 | — |
| PN-6 | .p8 文件上传 | 上传 .p8 文件 | Private Key 文本框填入文件内容 | — |
| PN-7 | 保存配置 | 填入测试值 → Save | 创建成功，列表显示 apns 类型 | — |

### 3.3 推送触发（需要 Apple Developer 账号 + 真机）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| PN-8 | 告警推送 | 创建 cpu ≥ 1% 告警 → 关联 APNs 通知组 → 等触发 | 手机收到推送通知 | — |
| PN-9 | 推送内容 | 查看推送内容 | 标题含 "[ServerBee]" + 服务器名，body 含告警消息 | — |
| PN-10 | 推送点击 | 点击推送通知 | App 打开并跳转到对应服务器详情页 | — |
| PN-11 | 前台通知 | App 在前台时触发告警 | Banner 通知显示 | — |

### 3.4 iOS 端推送注册

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| PN-12 | 权限请求 | 首次登录成功后 | 弹出通知权限请求 | — |
| PN-13 | 允许通知 | 允许权限 | device token 自动上报到服务端 | — |
| PN-14 | 拒绝通知 | 拒绝权限 | 不上报 token，App 正常使用 | — |

---

## 四、iOS App 核心功能

### 4.1 认证流程

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| APP-1 | 手动登录 | 填入服务器地址、用户名、密码 → Login | 登录成功，显示服务器列表 | — |
| APP-2 | URL 自动补全 | 输入 `192.168.1.1:9527`（无 scheme） | 自动加 `https://` | — |
| APP-3 | Session 恢复 | 杀掉 App → 重新打开 | 自动恢复登录状态（token refresh） | — |
| APP-4 | 登出 | 设置 → Logout | 回到登录页 | — |

### 4.2 服务器列表

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| APP-5 | 列表加载 | 登录后进入 Servers tab | 显示服务器卡片（名称、IP、CPU/Memory/Disk） | — |
| APP-6 | 实时更新 | 等待 WebSocket 推送 | 指标数据实时刷新（无需手动刷新） | — |
| APP-7 | 搜索 | 搜索框输入服务器名/IP | 列表过滤 | — |
| APP-8 | 在线/离线筛选 | 切换 Online/Offline/All | 列表正确过滤 | — |
| APP-9 | 下拉刷新 | 下拉服务器列表 | 重新获取数据 | — |
| APP-10 | 上线/下线 | 启动/停止 Agent | 服务器状态实时变化（绿色↔红色） | — |

### 4.3 服务器详情

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| APP-11 | 详情页 | 点击服务器卡片 | 显示 CPU、内存、磁盘、网络详细指标 | — |
| APP-12 | 历史图表 | 查看详情页图表区域 | 显示 1h/6h/24h/7d 历史趋势图 | — |
| APP-13 | 时间范围切换 | 切换不同时间范围 | 图表数据相应变化 | — |

### 4.4 告警列表

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| APP-14 | 告警列表 | 切换到 Alerts tab | 显示告警事件列表 | — |
| APP-15 | 告警详情 | 点击告警事件 | 显示规则名、服务器名、状态、触发次数 | — |
| APP-16 | 下拉刷新 | 下拉告警列表 | 重新获取最新告警 | — |
| APP-17 | 空状态 | 无告警时 | 显示 "No Alerts" 空状态 | — |

### 4.5 设置

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| APP-18 | 设置页 | 切换到 Settings tab | 显示外观设置和登出按钮 | — |
| APP-19 | 登出确认 | 点击 Logout | 弹出确认对话框 | — |

---

## 五、WebSocket & Token 刷新

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| WS-1 | WS 连接 | 登录后 | WS 连接到 `/api/ws/servers`，状态为 connected | — |
| WS-2 | WS 断线重连 | 断网 → 恢复网络 | 自动重连，指数退避 | — |
| WS-3 | Token 过期重连 | 等 15 分钟 | WS 断开 → 自动 refresh token → 重连 | — |
| WS-4 | Token 刷新竞态 | 同时触发 API 401 和 WS 重连 | 只发一次 refresh 请求（coalescing） | — |
| WS-5 | Refresh 失败 | Refresh token 过期/无效 | App 回到登录页 | — |

---

## 六、Swagger / OpenAPI

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| OA-1 | 新端点注册 | 访问 `/swagger-ui/` | 显示 mobile-auth tag 下所有端点 | — |
| OA-2 | Bearer 认证 | Swagger UI 的 Authorize 按钮 | 可选择 Bearer Token 认证方式 | — |
| OA-3 | 告警详情端点 | 搜索 alert-events | 包含 `GET /api/alert-events/{alert_key}` | — |
