# 02 首次登录引导(强制改密) — 冒烟测试

**前置条件**:全新数据库,使用默认/初始管理员凭据首次登录。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| O1 | 首次登录跳转引导 | 用初始凭据登录 | 进入 `/onboarding`,强制修改密码/用户名 | 是 | ✅ |
| O2 | 弱密码校验 | 输入过短/弱密码 | 提示不符合密码强度要求 | 否 | ✅ |
| O3 | 完成引导 | 设置新用户名+强密码 → 提交 | 跳转 `/`,后续用新凭据登录成功 | 是 | ✅ |
| O4 | 引导幂等 | 完成后重新登录 | 不再进入 onboarding,直达 `/` | 是 | ✅ |
| O5 | 未完成引导拦截 | 引导未完成时访问其他路由 | 被拦回 `/onboarding` | 否 | ✅ |

> ✅ O2(**已修复并真机端到端复验通过**): 新增 `AuthService::validate_password_strength`(最小 8 字符),在 `complete_onboarding` 与 `change_password` 两处用户自设密码边界强制校验;前端 onboarding 表单同步加最小长度校验 + 提示文案(中英)。回归测试 `auth.rs::test_complete_onboarding_rejects_weak_password` / `test_change_password_rejects_weak_password`(TDD 红/绿);server lib 508 全绿、前端 499 全绿、CI clippy/typecheck/lint 干净。**编排者真机端到端复验(重建 server + 前端后,隔离全新库 :9528):**
> - **T1 onboarding 后端边界(API)**:首登随机密码登录后 `POST /api/auth/onboarding` —— 弱密码 `"123"` → **422** `Validation error: Password must be at least 8 characters`,`/api/auth/me` 仍 `must_change_password:true`(未放行);7 位边界值 `"1234567"` → **422** 同样被拒;`"Strong#2026"`(≥8) → **200** `{"data":"ok"}`、`must_change_password:false`、用新密码可重新登录、旧随机密码登录返回 `UNAUTHORIZED`。
> - **T2 onboarding 前端表单(浏览器)**:全新库 UI 登录跳 `/onboarding`;EN 输入 `123` 提交 → toast `Password must be at least 8 characters.`、停留 `/onboarding`、未发请求,密码框下有提示 `Use at least 8 characters.`;切 ZH 重载后提示 `至少 8 个字符。`、输入 `123` 提交 → toast `密码至少需要 8 个字符。`、仍停留 `/onboarding`;输入强密码 `Strong#2026` 提交 → 跳转 `/`。
> - **T3 change_password 边界(同源策略回归)**:已登录用户 `PUT /api/auth/password` —— 弱新密码 `"123"` / 7 位 `"1234567"` 均 **422** 被拒、旧密码仍可登录;强新密码 `"NewStrong#99"` → **200**,新密码可登录、旧密码返回 `UNAUTHORIZED`。
> 备注: 1.0.0 行为为首次启动生成随机管理员密码并强制 onboarding(`SERVERBEE_ADMIN__PASSWORD` 不再直接生效),O1/O3/O4/O5 据此真实验证通过。

**汇总**:✅ 5 / ❌ 0 / — 0(O2 弱密码校验已修复,T1/T2/T3 真机端到端复验全部 PASS)
