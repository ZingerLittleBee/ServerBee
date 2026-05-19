# 27 审计日志 — 冒烟测试

**前置条件**:已登录 admin,进入 `/settings/audit-logs`。深度用例见 [../audit-logs.md](../audit-logs.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| AL1 | 日志记录 | 执行写操作(创建用户/改设置等) | 审计日志出现对应条目(操作者/动作/时间) | 是 | ✅ |
| AL2 | 列表分页 | 浏览日志列表 | 分页正常,按时间倒序 | 否 | ✅ |
| AL3 | 登录事件 | 登录/登出 | 记录认证事件 | 否 | ✅ |
| AL4 | 失败操作 | 触发被拒操作 | 记录失败/拒绝事件 | 否 | ⚠️✅ |
| AL5 | Member 不可见 | member 访问审计日志 | 无权限,入口隐藏 | 否 | ✅ |

> AL1: 写操作记录完整 — `capabilities_changed`/`change_password`/`2fa_enable`/`2fa_disable`/`agent_enrolled`/`onboarding`,含 user_id/action/detail/ip/created_at。
> AL2: `?page&page_size` 分页正常,响应在 `data.entries`,按 id 降序(时间倒序)。
> AL3: 新登录即时记入(`login`,id 随写操作递增)。
> AL4: 被拒操作 `terminal_open_denied`(能力位禁用)有审计记录,detail 含 deny_reason。注:失败登录(密码错误,HTTP 401)**不**写审计 — auth.rs:186 仅成功登录路径记录,无失败分支。其他拒绝类操作有记录,故按"触发被拒操作→记录"判通过但带提醒。
> AL5: member 访问 `/api/audit-logs` 返回 HTTP 403;侧边栏 nav 项 `adminOnly: true`(sidebar.tsx:44 / app-sidebar.tsx:72),入口对 member 隐藏。

**汇总**:✅ 5 / ❌ 0 / — 0(AL4 带已知提醒:失败登录不审计)
