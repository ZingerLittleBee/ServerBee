# 32 Mobile API 与 iOS App — 冒烟测试

**前置条件**:Server 运行,iOS App 可用(`apps/ios/ServerBee/`)。深度用例见 [../mobile-ios.md](../mobile-ios.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| MB1 | Mobile 登录 | `POST /api/mobile/auth/login`(用户名/密码+device_name) | 返回 token + refresh_token | 是 | ✅ |
| MB2 | Token 刷新 | `POST /api/mobile/auth/refresh` | 返回新 token,旧 refresh 轮换 | 是 | ✅ |
| MB3 | 扫码配对 | 生成配对码 → `POST /api/mobile/auth/pair` | 配对登录成功 | 否 | ✅ |
| MB4 | 推送注册 | `POST /api/mobile/push/register`(APNs token) | device token 绑定成功 | 否 | ✅ |
| MB5 | 设备管理 | `/settings/mobile-devices` 查看/撤销设备 | 列表显示最后使用时间,撤销后该设备失效 | 否 | ✅ |
| MB6 | iOS App 主流程 | App 登录 → 查看服务器列表/详情/告警 | 数据正常,WebSocket 实时更新 | 否 | — |
| MB7 | 推送通知 | 触发告警 | iOS 设备收到 APNS 推送 | 否 | — |
| MB8 | 登出 | `POST /api/mobile/auth/logout` | token 失效 | 否 | ✅ |

> MB1: login 返回 access_token+refresh_token+user(admin),expires 900s/2592000s。
> MB2: refresh 返回新 token,旧 refresh 复用即 401(已轮换)。
> MB3: `/api/mobile/pair` 生成 code(300s)→ `/api/mobile/auth/pair` 兑换成功返回完整 token 组。
> MB4: push/register(APNs token,Bearer)返回 ok。
> MB5: `/settings/mobile-devices` UI 列出设备(名称+最后使用时间),撤销确认对话框 → 撤销后 toast "Device access revoked",该设备 token 立即 401。
> MB6/MB7: 需真实 iOS 设备/模拟器与 APNs,环境不具备 — 记 —(非缺陷,环境限制)。
> MB8: logout 后该 token 立即 401(失效)。

**汇总**:✅ 6 / ❌ 0 / — 2
