# Security 设置页测试用例

## 前置条件

参照 [README.md](README.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

创建 member 用户用于权限测试：
```bash
curl -s -b /tmp/sb-cookies.txt -X POST http://localhost:9527/api/users \
  -H 'Content-Type: application/json' -d '{"username":"viewer","password":"viewer123","role":"member"}'
```

---

## 一、页面加载与渲染（/settings/security）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SC-1 | 页面正常加载 | 登录后导航到 `/settings/security` | 页面加载完成，显示标题 "Security" | ✅ |
| SC-2 | 三个区域渲染 | 查看页面 | 显示三个 Card 区域：Two-Factor Authentication、Change Password、Linked Accounts | ✅ |
| SC-3 | 侧边栏导航 | 点击侧边栏 "Security" 链接 | 导航到 `/settings/security` | ✅ |
| SC-4 | Member 用户可访问 | 以 viewer 用户登录 → 访问 `/settings/security` | 页面正常加载，所有功能可用（Security 是个人设置，非 admin-only） | — |

---

## 二、修改密码

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| PW-1 | 表单渲染 | 查看 Change Password 区域 | 显示 "Current Password" 和 "New Password" 两个密码输入框 + "Change Password" 提交按钮 | ✅ |
| PW-2 | 密码字段类型 | 查看输入框 type | 两个输入框均为 `type="password"`（掩码显示） | ✅ |
| PW-3 | 正确修改密码 | 输入当前密码 "admin123" → 新密码 "newpass888" → 提交 | toast 显示 "Password changed"，表单清空 | ✅ API 验证密码已修改成功 |
| PW-4 | 用新密码登录 | 登出 → 使用新密码 "newpass888" 登录 | 登录成功，跳转到 Dashboard | ✅ API 验证 login 返回 200 |
| PW-5 | 旧密码失效 | 登出 → 使用旧密码 "admin123" 登录 | 登录失败，显示错误信息 | ✅ API 返回 UNAUTHORIZED |
| PW-6 | 恢复原密码 | 使用 "newpass888" 登录 → Security → 修改回 "admin123" | 修改成功 | ✅ |
| PW-7 | 当前密码错误 | 输入错误的当前密码 → 新密码 → 提交 | 显示红色错误文字，密码未修改 | ✅ 显示 "Current password is incorrect" |
| PW-8 | 新密码为空 | 输入当前密码 → 新密码留空 → 提交 | 表单不提交（HTML required 校验） | ✅ |
| PW-9 | 新密码过短 | 输入当前密码 → 新密码输入 3 个字符 → 提交 | 表单不提交（HTML minLength=8 校验） | ✅ 代码确认 minLength=8 |
| PW-10 | 提交按钮 pending 态 | 点击提交后 | 按钮文字变为 "Changing..."（isPending 状态） | ✅ 代码逻辑验证 |

---

## 三、双因素认证 (2FA/TOTP)

### 3.1 初始状态（未启用）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| TF-1 | 2FA 未启用状态 | 查看 Two-Factor Authentication 区域 | 显示描述文字 + "Set Up 2FA" 按钮 | ✅ |
| TF-2 | 无绿色状态标识 | 查看区域 | 无 Shield 图标和 "2FA is enabled" 文字 | ✅ |

### 3.2 设置 2FA 流程

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| TF-3 | 发起设置 | 点击 "Set Up 2FA" 按钮 | 显示 QR 码图片 + 手动输入密钥（details/summary 折叠） + 6 位验证码输入框 + "Verify & Enable" 按钮 + "Cancel" 按钮 | ✅ |
| TF-4 | QR 码渲染 | 查看 QR 码区域 | 显示 base64 PNG 图片，白色背景，192x192 尺寸 | ✅ src="data:image/png;base64,..." |
| TF-5 | 手动密钥显示 | 点击 "Can't scan?" 展开 | 显示 TOTP secret（等宽字体，可复制） | ✅ 1 个 details + 1 个 code 元素 |
| TF-6 | 验证码输入限制 | 在验证码框中输入 | 仅接受数字（非数字字符被过滤），最大 6 位 | ✅ 输入 "abc123" 实际值为 "123" |
| TF-7 | 按钮禁用状态 | 输入少于 6 位数字 | "Verify & Enable" 按钮为 disabled | ✅ |
| TF-8 | 取消设置 | 点击 "Cancel" 按钮 | 返回初始未启用状态，QR 码和输入框消失 | ✅ 回到 "Set Up 2FA" |
| TF-9 | 错误验证码 | 输入错误的 6 位数字 → 提交 | 显示红色错误文字 "Invalid code" | ✅ "Invalid code. Please try again." |

### 3.3 2FA 启用后状态

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| TF-10 | 启用后状态显示 | 2FA 启用后查看区域 | 显示绿色 Shield 图标 + "2FA is enabled" 文字 + "Disable 2FA" 按钮（红色） | ⏭️ 需要真实 TOTP authenticator |
| TF-11 | 设置按钮消失 | 2FA 启用后 | "Set Up 2FA" 按钮不再显示 | ⏭️ 需要真实 TOTP authenticator |

### 3.4 禁用 2FA

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| TF-12 | 展开禁用表单 | 点击 "Disable 2FA" 按钮 | 显示密码输入框 + "Confirm Disable" 按钮（红色）+ "Cancel" 按钮 | ⏭️ 需要先启用 2FA |
| TF-13 | 取消禁用 | 点击 "Cancel" 按钮 | 密码输入消失，回到启用状态显示 | ⏭️ 需要先启用 2FA |
| TF-14 | 正确密码禁用 | 输入当前密码 → 点击 "Confirm Disable" | toast 显示 "2FA disabled"，状态回到未启用 | ⏭️ 需要先启用 2FA |
| TF-15 | 错误密码禁用 | 输入错误密码 → 点击 "Confirm Disable" | 显示红色错误文字，2FA 保持启用 | ⏭️ 需要先启用 2FA |

---

## 四、OAuth 关联账户

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| OA-1 | 无关联账户 | 未关联任何 OAuth 账户时查看 | 显示 "No linked OAuth accounts" 文字 | ✅ |
| OA-2 | 加载骨架屏 | 首次加载 Linked Accounts 区域 | 加载时显示 Skeleton 占位 | ⏭️ 本地加载过快 |

---

## 五、API 端点验证

### 5.1 密码修改 API

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| API-1 | 正确修改密码 | `PUT /api/auth/password` with `{"old_password":"admin123","new_password":"test1234"}` | 200，返回 `{"data":"ok"}` | ✅ |
| API-2 | 恢复密码 | `PUT /api/auth/password` with `{"old_password":"test1234","new_password":"admin123"}` | 200 | ✅ |
| API-3 | 旧密码错误 | `PUT /api/auth/password` with `{"old_password":"wrong","new_password":"test1234"}` | 400 Bad Request | ✅ |
| API-4 | 新密码为空 | `PUT /api/auth/password` with `{"old_password":"admin123","new_password":""}` | 422 Unprocessable Entity | ✅ |
| API-5 | 未认证 | 不带 cookie 调用 `PUT /api/auth/password` | 401 | ✅ |

### 5.2 2FA API

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| API-6 | 查询 2FA 状态 | `GET /api/auth/2fa/status` | 200，返回 `{"data":{"enabled":false}}` | ✅ |
| API-7 | 发起 2FA 设置 | `POST /api/auth/2fa/setup` | 200，返回含 secret, otpauth_url, qr_code_base64 三个字段 | ✅ |
| API-8 | 无 pending 时启用 | 消耗 setup 后 → `POST /api/auth/2fa/enable` with `{"code":"123456"}` | 401 Unauthorized (无 pending secret) | ✅ |
| API-9 | 未认证 | 不带 cookie → `GET /api/auth/2fa/status` | 401 | ✅ |

### 5.3 OAuth API

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| API-10 | 查询 OAuth providers | `GET /api/auth/oauth/providers` | 200，返回 `{"data":{"providers":[]}}` (未配置时为空) | ✅ |
| API-11 | 查询关联账户 | `GET /api/auth/oauth/accounts` | 200，返回 `{"data":[]}` | ✅ |
| API-12 | 解绑不存在的账户 | `DELETE /api/auth/oauth/accounts/nonexistent` | 404 Not Found | ✅ |
| API-13 | 未配置的 provider | `GET /api/auth/oauth/notexist` | 400 Bad Request | ✅ |

---

## 六、i18n 国际化

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| I18N-1 | 英文模式 | 英文下查看 Security 页面 | "Security"、"Two-Factor Authentication"、"Change Password"、"Linked Accounts" 英文显示 | ✅ |
| I18N-2 | 中文模式 | 切换中文 | "安全"、"双因素认证"、"更改密码"、"关联账户"、"暂无关联的 OAuth 账户" 中文显示 | ✅ |

---

## 测试统计

| 模块 | 用例数 | ✅ | ⏭️ | — |
|------|--------|-----|------|-----|
| 页面加载与渲染 | 4 | 3 | 0 | 1 |
| 修改密码 | 10 | 10 | 0 | 0 |
| 双因素认证 (2FA) | 15 | 9 | 6 | 0 |
| OAuth 关联账户 | 2 | 1 | 1 | 0 |
| API 端点验证 | 13 | 13 | 0 | 0 |
| i18n | 2 | 2 | 0 | 0 |
| **合计** | **46** | **38** | **7** | **1** |

- ✅ 通过：38 (82.6%)
- ⏭️ 跳过（环境限制 — 需要真实 TOTP authenticator 或 OAuth provider）：7 (15.2%)
- — 未测（需要 member 用户登录测试）：1 (2.2%)
