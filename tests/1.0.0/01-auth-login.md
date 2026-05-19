# 01 登录 / 登出 / 会话 — 冒烟测试

**前置条件**:参照 [../README.md](../README.md) 启动 Server,管理员账户已就绪。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| L1 | 正确凭据登录 | `/login` 输入 admin 用户名/密码 → 提交 | 跳转 `/`,顶部显示当前用户 | 是 | ✅ |
| L2 | 错误密码 | 输入错误密码 → 提交 | 提示凭据错误,停留在登录页 | 是 | ✅ |
| L3 | 会话保持 | 登录后刷新页面 | 仍为登录态,无需重新登录 | 是 | ✅ |
| L4 | 登出 | 点击登出 | 跳转 `/login`,Cookie 清除 | 是 | ✅ |
| L5 | 未登录访问受保护路由 | 直接访问 `/servers` | 重定向到 `/login` | 是 | ✅ |
| L6 | 登录限流 | 连续多次错误登录(同 IP) | 触发 15 分钟窗口限流提示 | 否 | ✅ |
| L7 | Secure Cookie | 生产配置下登录 | Cookie 带 Secure/HttpOnly 标志 | 否 | — |

> ✅ L4(**阻断级**,已修复): 根因为 `app-sidebar.tsx` 的 logout `DropdownMenuItem` 使用了 Radix 的 `onSelect`,而本项目 `DropdownMenuItem` 基于 Base UI `Menu.Item`,其动作回调是 `onClick`,`onSelect` 被静默忽略 → 点击无任何效果。已改为 `onClick`,并补充回归测试 `app-sidebar.test.tsx`(编码 Base UI Menu.Item 契约)。同类 bug 在 `network-probes.tsx` 的编辑/删除下拉项一并修复。全量前端测试 499 通过、typecheck/lint 干净。
> — L7: 本轮以 `SERVERBEE_AUTH__SECURE_COOKIE=false` 开发配置启动,未验证生产 Secure 标志。
> 备注 L2: 凭据错误以原始 JSON `{"error":{"code":"UNAUTHORIZED",...}}` 形式弹出 toast,功能正确但文案未本地化(非阻断 UX)。
> 备注 L6: 同 IP 连续错误登录返回 429,限流生效;副作用是后续浏览器登录在 15 分钟窗口内被锁,导致部分需 member 登录的用例本轮改用 API 验证。

**汇总**:✅ 6 / ❌ 0 / — 1
