# 02 首次登录引导(强制改密) — 冒烟测试

**前置条件**:全新数据库,使用默认/初始管理员凭据首次登录。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| O1 | 首次登录跳转引导 | 用初始凭据登录 | 进入 `/onboarding`,强制修改密码/用户名 | 是 | ✅ |
| O2 | 弱密码校验 | 输入过短/弱密码 | 提示不符合密码强度要求 | 否 | ✅ |
| O3 | 完成引导 | 设置新用户名+强密码 → 提交 | 跳转 `/`,后续用新凭据登录成功 | 是 | ✅ |
| O4 | 引导幂等 | 完成后重新登录 | 不再进入 onboarding,直达 `/` | 是 | ✅ |
| O5 | 未完成引导拦截 | 引导未完成时访问其他路由 | 被拦回 `/onboarding` | 否 | ✅ |

> ✅ O2(已修复): 新增 `AuthService::validate_password_strength`(最小 8 字符),在 `complete_onboarding` 与 `change_password` 两处用户自设密码边界强制校验;前端 onboarding 表单同步加最小长度校验 + 提示文案(中英)。回归测试 `auth.rs::test_complete_onboarding_rejects_weak_password` / `test_change_password_rejects_weak_password`(TDD 红/绿);server lib 508 全绿、前端 499 全绿、CI clippy/typecheck/lint 干净。
> 备注: 1.0.0 行为为首次启动生成随机管理员密码并强制 onboarding(`SERVERBEE_ADMIN__PASSWORD` 不再直接生效),O1/O3/O4/O5 据此真实验证通过。

**汇总**:✅ 5 / ❌ 0 / — 0(O2 弱密码校验已修复并补回归测试,待真机复验)
