# Web 终端测试用例

## 前置条件

参照 [README.md](README.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

Agent 需启用 CAP_TERMINAL 能力：
```bash
# 获取 server_id
SERVER_ID=$(curl -s -b /tmp/sb-cookies.txt http://localhost:9527/api/servers | python3 -c "import sys,json; print(json.load(sys.stdin)['data'][0]['id'])")

# 启用 Terminal capability (capabilities |= 1)
curl -s -b /tmp/sb-cookies.txt -X PUT "http://localhost:9527/api/servers/$SERVER_ID" \
  -H 'Content-Type: application/json' \
  -d "{\"capabilities\":57}"
# 57 = CAP_TERMINAL(1) + CAP_PING_ICMP(8) + CAP_PING_TCP(16) + CAP_PING_HTTP(32)
```

---

## 一、页面加载与渲染（/terminal/:serverId）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| TM-1 | 页面正常加载 | 导航到 `/terminal/:serverId` | 页面加载完成，显示 "Terminal" 标题 + "18c893b9..." | ✅ |
| TM-2 | 返回按钮 | 查看顶部工具栏 | 左侧显示 "Back" 按钮（ArrowLeft 图标） | ✅ |
| TM-3 | xterm.js 容器 | 查看页面主体 | 渲染 xterm.js 终端容器，显示 fish shell 欢迎信息 | ✅ |

---

## 二、连接状态

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| TM-4 | 自动连接 | 进入终端页面 | 自动发起 WebSocket 连接，状态变为 "connected" | ✅ 显示 fish shell prompt |
| TM-5 | 连接状态指示器 | 查看工具栏右侧 | 显示绿色圆点(bg-green-500) + "connected" 文字 | ✅ |
| TM-6 | 断线重连按钮 | 连接断开后 | 状态变为红色 "Closed" + "Reconnect" 按钮出现 | ⏭️ 需手动断线 |
| TM-7 | 点击重连 | 点击 "Reconnect" 按钮 | 重新发起 WS 连接 | ⏭️ 需手动断线 |
| TM-8 | 错误信息 | 连接失败时 | 工具栏右侧显示红色错误文字 | ✅ 显示 "WebSocket connection failed" (初始连接重试) |

---

## 三、终端交互

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| TM-9 | 命令输入输出 | 连接成功后输入 `echo hello` + 回车 | 终端显示 "hello" 输出 | ✅ |
| TM-10 | 命令历史 | 按上箭头 | 显示上一条命令 | ⏭️ xterm.js 键盘事件难以模拟 |
| TM-11 | Tab 补全 | 输入 `ls /` + Tab | 显示路径补全建议 | ⏭️ xterm.js 键盘事件难以模拟 |

---

## 四、能力控制

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| TM-12 | CAP_TERMINAL 启用 | 服务器启用 Terminal capability (caps=57) | Server Detail 页显示 Terminal 按钮 | ✅ |
| TM-13 | CAP_TERMINAL 禁用 | 服务器禁用 Terminal capability (caps=56) | Server Detail 页不显示 Terminal 按钮 | ✅ |
| TM-14 | 服务器离线 | Agent 离线 | Terminal 按钮不显示 | ✅ server-detail.md SV-17 已验证 |

---

## 五、i18n

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| I18N-1 | 英文模式 | 英文下查看 | "Terminal"、"Back"、"connected" 英文显示 | ✅ |
| I18N-2 | 中文模式 | 切换中文 | 显示中文翻译 | — |

---

## 测试统计

| 模块 | 用例数 | ✅ | ⏭️ | — |
|------|--------|-----|------|-----|
| 页面加载与渲染 | 3 | 3 | 0 | 0 |
| 连接状态 | 5 | 3 | 2 | 0 |
| 终端交互 | 3 | 1 | 2 | 0 |
| 能力控制 | 3 | 3 | 0 | 0 |
| i18n | 2 | 1 | 0 | 1 |
| **合计** | **16** | **11** | **4** | **1** |

- ✅ 通过：11 (68.8%)
- ⏭️ 跳过（需手动断线或 xterm.js 键盘模拟受限）：4 (25%)
- — 未测：1 (6.2%)
