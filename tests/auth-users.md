# 认证、用户与安全测试用例

## 前置条件

参照 [README.md](README.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

创建 member 用户用于权限测试：
```bash
curl -s -b /tmp/sb-cookies.txt -X POST http://localhost:9527/api/users \
  -H 'Content-Type: application/json' -d '{"username":"viewer","password":"viewer123","role":"member"}'
```

---

## 一、登录/登出（/login）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| AU-1 | 错误密码提示 | 输入错误密码 | 显示 "Unauthorized" 错误文本 | ✅ |
| AU-2 | 正确登录跳转 | 输入正确密码 | 跳转到 `/` (Dashboard) | ✅ |
| AU-3 | 登出回到登录页 | 点击 Log out | 跳转到 `/login` | ✅ |

---

## 二、用户管理（/settings/users）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| AU-4 | 创建用户 | Add User → 填写 username/password → Create | 列表出现新用户 | ✅ |
| AU-5 | 删除用户 | 删除 testuser | 列表仅剩 admin | ✅ |

---

## 三、通知渠道（/settings/notifications）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| AU-6 | 通知渠道展示 | 创建 Webhook 通知渠道 | 列表显示名称和类型 | ✅ |

---

## 四、API Keys（/settings/api-keys）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| AU-7 | API Key 展示 | 创建 API Key | Active Keys 显示 prefix 和创建日期 | ✅ |

---

## 五、公共状态页（/status）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| AU-8 | 公共状态页 | 无需登录访问 `/status` | 显示服务器卡片和指标 | ✅ |
