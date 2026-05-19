# 04 用户管理与 RBAC — 冒烟测试

**前置条件**:已登录 admin,进入 `/settings/users`。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| U1 | 创建用户 | 新建 member 用户(用户名/密码/角色) | 列表出现新用户 | 是 | ✅ |
| U2 | 创建 admin 用户 | 新建 admin 角色用户 | 创建成功,角色标记 admin | 否 | ✅ |
| U3 | 编辑用户 | 修改用户名/角色 → 保存 | 更新生效 | 否 | ✅ |
| U4 | 删除用户 | 删除非自身用户 → 确认 | 用户移除 | 否 | ✅ |
| U5 | Member 只读 | 用 member 登录,访问写操作页(用户管理/设置) | 写按钮隐藏或操作被拒(403) | 是 | ✅ |
| U6 | Member 可查看 | member 浏览仪表盘/服务器详情 | 数据正常展示 | 是 | ✅ |
| U7 | 自删保护 | 尝试删除当前登录用户 | 被阻止 | 否 | ✅ |

> ✅ U1/U2/U3/U4: 创建 member、创建 admin(role=admin)、编辑角色(admin→member)、删除用户均返回 200。
> ✅ U5: member 登录后写操作被拒——`POST /api/users`→403、`POST /api/agent/enrollments`→403,`require_admin` 生效。
> ✅ U6: member 可读——`GET /api/servers`→200、`GET /api/auth/me`→200。
> ✅ U7: 删除当前登录管理员被阻止,返回 400 `Cannot delete the last admin user`(自删保护生效)。
> 备注: 本轮提高 `SERVERBEE_RATE_LIMIT__LOGIN_MAX=100` 并把 L6 限流用例放最后,member 登录得以在限流窗口外正常验证。

**汇总**:✅ 7 / ❌ 0 / — 0
