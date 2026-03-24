# Ping Tasks 测试用例

## 前置条件

> 通用环境搭建参照 [README.md](README.md) 中的「启动本地环境」部分。以下为完整独立步骤：

```bash
# 0. 为本轮测试准备隔离环境（避免复用旧数据库和旧 cookie）
TMP_ROOT="$(mktemp -d /tmp/serverbee-ping-tasks.XXXXXX)"
ADMIN_COOKIE="$TMP_ROOT/admin-cookies.txt"
MEMBER_COOKIE="$TMP_ROOT/member-cookies.txt"
MEMBER_USER="viewer_$(date +%s)"
MEMBER_PASS="viewer123"

# 1. 构建前端资源
cd apps/web && bun install && bun run build && cd ../..

# 2. 启动 Server（独立 data dir，避免污染已有开发数据）
SERVERBEE_SERVER__DATA_DIR="$TMP_ROOT/data" \
SERVERBEE_ADMIN__PASSWORD=admin123 \
SERVERBEE_AUTH__SECURE_COOKIE=false \
cargo run -p serverbee-server &
SERVER_PID=$!

# 3. 等待 Server 就绪
until curl -fsS http://localhost:9527/healthz >/dev/null; do
  sleep 1
done

# 4. 登录 admin，获取会话和 auto-discovery key
curl -fsS -c "$ADMIN_COOKIE" -X POST http://localhost:9527/api/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin123"}'
KEY=$(curl -fsS -b "$ADMIN_COOKIE" http://localhost:9527/api/settings/auto-discovery-key \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['key'])")

# 5. 启动 Agent
SERVERBEE_SERVER_URL="http://127.0.0.1:9527" \
SERVERBEE_AUTO_DISCOVERY_KEY="$KEY" \
cargo run -p serverbee-agent &
AGENT_PID=$!

# 6. 等待 Agent 注册上线
until curl -fsS -b "$ADMIN_COOKIE" http://localhost:9527/api/servers \
  | python3 -c "import sys,json; raise SystemExit(0 if len(json.load(sys.stdin)['data']) > 0 else 1)"; do
  sleep 1
done

# 7. 创建并登录 member 用户（用于 RBAC 测试）
curl -fsS -b "$ADMIN_COOKIE" -X POST http://localhost:9527/api/users \
  -H 'Content-Type: application/json' \
  -d "{\"username\":\"$MEMBER_USER\",\"password\":\"$MEMBER_PASS\",\"role\":\"member\"}"
curl -fsS -c "$MEMBER_COOKIE" -X POST http://localhost:9527/api/auth/login \
  -H 'Content-Type: application/json' \
  -d "{\"username\":\"$MEMBER_USER\",\"password\":\"$MEMBER_PASS\"}"

# 8. 打开浏览器
agent-browser open http://localhost:9527/login
```

- 除第十节国际化用例外，默认以英文界面执行测试。开始前若界面不是英文，先切换为 English 再执行第 1 到第 9 节。
- API 测试统一使用会话文件：Admin 使用 `"$ADMIN_COOKIE"`，Member 使用 `"$MEMBER_COOKIE"`，未登录场景不携带任何 Cookie 或 API Key。
- 测试结束后可执行 `kill "$AGENT_PID" "$SERVER_PID"`；如需清理临时文件，再执行 `rm -rf "$TMP_ROOT"`。

---

## 一、页面加载与基础渲染

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| PT-1 | 页面正常加载 | 登录后导航到 `/settings/ping-tasks` | 页面加载完成，显示标题 "Ping Tasks" 和 "Probe Tasks" 卡片 | — |
| PT-2 | 空状态显示 | 无任何 ping task 时访问页面 | 显示居中文字提示 "No ping tasks configured" | — |
| PT-3 | 加载骨架屏 | 页面首次加载（或网络慢速时） | 显示 2 个 Skeleton 占位条 | — |
| PT-4 | 侧边栏导航 | 点击侧边栏 "Ping Tasks" 链接 | 导航到 `/settings/ping-tasks` | — |

---

## 二、创建 Ping Task

### 2.1 表单显示与隐藏

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| CR-1 | 打开创建表单 | 点击右上角 "Add" 按钮 | 表单区域展开，显示 Name、Probe Type、Interval、Target 输入框和 Server 勾选列表 | — |
| CR-2 | 收起创建表单 | 再次点击 "Add" 按钮 | 表单收起隐藏 | — |
| CR-3 | Cancel 按钮 | 填写部分内容后点击 "Cancel" | 表单关闭，所有字段重置为默认值 | — |
| CR-4 | 默认值 | 打开表单 | Probe Type 默认 ICMP，Interval 默认 60，Server 不勾选（代表所有服务器）。表单无 enabled 开关，创建时固定为 enabled=true | — |

### 2.2 ICMP Probe

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| CR-5 | 创建 ICMP 任务 | 选择 ICMP → Name 填 "Google DNS" → Target 填 "8.8.8.8" → 点击 "Create" | toast 提示 "Ping task created"，列表新增一条记录，表单收起 | — |
| CR-6 | ICMP placeholder | 选择 ICMP 类型 | Target 输入框 placeholder 显示 "e.g. 8.8.8.8 or google.com" | — |

### 2.3 TCP Probe

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| CR-7 | 创建 TCP 任务 | 选择 TCP → Name 填 "SSH Check" → Target 填 "example.com:22" → 点击 "Create" | 创建成功，列表显示 TCP 类型标签 | — |
| CR-8 | TCP placeholder | 选择 TCP 类型 | Target 输入框 placeholder 显示 "e.g. google.com:443" | — |

### 2.4 HTTP Probe

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| CR-9 | 创建 HTTP 任务 | 选择 HTTP → Name 填 "Website" → Target 填 "https://example.com" → 点击 "Create" | 创建成功，列表显示 HTTP 类型标签 | — |
| CR-10 | HTTP placeholder | 选择 HTTP 类型 | Target 输入框 placeholder 显示 "e.g. https://google.com" | — |

### 2.5 Server 选择

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| CR-11 | 不选服务器（全部） | 不勾选任何 server → 创建 | 创建成功，列表条目显示 "all servers" | — |
| CR-12 | 选择指定服务器 | 勾选 1 台 server → 创建 | 创建成功，列表条目显示 "1 server(s)" | — |
| CR-13 | 选择多台服务器 | 勾选 2+ 台 server → 创建 | 创建成功，列表条目显示对应服务器数量 | — |
| CR-14 | 无服务器时不显示选择框 | 删除所有服务器后打开创建表单 | Server 勾选 fieldset 不显示 | — |
| CR-15 | 不存在的 server ID | 通过 API 创建，server_ids 含不存在的 ID | 创建成功（无 FK 校验），但任务不会同步到任何 Agent | — |

### 2.6 自定义 Interval

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| CR-16 | 自定义间隔 | Interval 修改为 30 → 创建 | 创建成功，列表显示 "30s" | — |
| CR-17 | 最小间隔 HTML 提示 | Interval 输入框通过 spinner 递减 | HTML min=5 限制 spinner 最小值为 5，但用户可手动输入更小值 | — |
| CR-18 | 极小 interval（API） | 通过 API 传 interval=1 创建 | 服务端无校验，创建成功。Agent 侧钳位为 max(interval, 5)，实际最快 5 秒一次 | — |

### 2.7 表单校验

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| CR-19 | Name 为空 | Name 留空 → 点击 "Create" | 表单不提交（HTML required 校验） | — |
| CR-20 | Target 为空 | Target 留空 → 点击 "Create" | 表单不提交（HTML required 校验） | — |
| CR-21 | Name 只有空格 | Name 输入 "   " → 点击 "Create" | 表单不提交（JS `trim()` 检查长度为 0，阻止提交） | — |

---

## 三、任务列表展示

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| LS-1 | 任务条目内容 | 查看列表中任意条目 | 显示：绿色/灰色活动图标、任务名称、Probe 类型、Target、Interval(秒)、Server 范围 | — |
| LS-2 | Enabled 任务图标 | 查看已启用任务 | Activity 图标为绿色（text-green-500） | — |
| LS-3 | Disabled 任务图标 | 查看已禁用任务 | Activity 图标为灰色（text-muted-foreground），名称旁显示 "(disabled)" | — |
| LS-4 | 多条任务排列 | 创建 3+ 条任务 | 列表以分割线分隔，垂直排列所有条目 | — |
| LS-5 | 任务操作按钮 | 查看条目右侧 | 显示 3 个按钮：图表展开、Enable/Disable 切换、删除（红色） | — |

---

## 四、启用/禁用 Ping Task

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| TG-1 | 禁用任务 | 点击已启用任务的 "Disable" 按钮 | toast 提示 "Ping task disabled"，图标变灰，按钮文字变为 "Enable"，名称旁出现 "(disabled)" | — |
| TG-2 | 启用任务 | 点击已禁用任务的 "Enable" 按钮 | toast 提示 "Ping task enabled"，图标变绿，按钮文字变为 "Disable"，"(disabled)" 标记消失 | — |
| TG-3 | Toggle pending 态 | 连续快速点击 Enable/Disable 2 次 | 按钮会在 `toggleMutation.isPending` 生效后变为 disabled，但在重渲染前仍可能发出多个并发 PUT；不应视为严格防重复点击保护 | — |

---

## 五、删除 Ping Task

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| DL-1 | 删除任务 | 点击删除按钮（红色 Trash2 图标） | toast 提示 "Ping task deleted"，任务从列表移除 | — |
| DL-2 | 删除最后一条 | 只剩 1 条任务时删除 | 列表变为空状态 "No ping tasks configured" | — |
| DL-3 | 删除按钮 pending 态 | 点击删除后立即观察 | **所有行**的删除按钮同时变为 disabled（共享 `deleteMutation.isPending`），防止并发删除 | — |
| DL-4 | 删除后数据清理 | 删除任务后，通过 API 查询该任务的 records | 对应 ping_record 记录也被删除 | — |

---

## 六、查看 Ping Records 图表

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| CH-1 | 展开图表 | 点击任务条目的图表按钮（BarChart3 图标） | 任务下方展开图表区域，显示 24 小时内的探测数据 | — |
| CH-2 | 收起图表 | 再次点击图表按钮 | 图表区域收起 | — |
| CH-3 | 仅展开一个 | 展开 Task A 图表后点击 Task B 的图表按钮 | Task A 图表收起，Task B 图表展开（同时只展开一个） | — |
| CH-4 | 统计摘要 | 查看展开的图表上方 | 显示三项统计：成功率（xx.x%）、平均延迟（xx.xms）、记录数 | — |
| CH-5 | AreaChart 渲染 | 等待图表数据加载 | 显示时间轴折线面积图，X 轴为 HH:mm 格式时间，Y 轴为延迟(ms) | — |
| CH-6 | Tooltip 交互 | 鼠标悬浮图表数据点 | 显示 Tooltip 弹出框，包含时间和延迟值（带 ms 单位） | — |
| CH-7 | 无记录时显示 | 展开刚创建（无数据）任务的图表 | 显示 "No records in the last 24 hours" 提示文字 | — |
| CH-8 | 加载态 | 展开图表加载过程中 | 显示 Skeleton 占位（h-48） | — |
| CH-9 | 失败记录处理 | 有探测失败记录时 | 图表中失败点映射为 null，线条在该点断开（`connectNulls={false}`），不显示为 0 | — |

---

## 七、权限控制 (RBAC)

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| RB-1 | Admin 完整访问 | 以 admin 登录访问 `/settings/ping-tasks` | 可查看、创建、切换启禁、删除所有 ping task | — |
| RB-2 | Member 只读 — 创建 | 携带 `"$MEMBER_COOKIE"` → 尝试 `POST /api/ping-tasks` | 返回 403 Forbidden | — |
| RB-3 | Member 只读 — 更新 | 携带 `"$MEMBER_COOKIE"` → 尝试 `PUT /api/ping-tasks/{id}` | 返回 403 Forbidden | — |
| RB-4 | Member 只读 — 删除 | 携带 `"$MEMBER_COOKIE"` → 尝试 `DELETE /api/ping-tasks/{id}` | 返回 403 Forbidden | — |
| RB-5 | Member 可读取列表 | 携带 `"$MEMBER_COOKIE"` → `GET /api/ping-tasks` | 返回 200，正常获取任务列表 | — |
| RB-6 | Member 可查看记录 | 携带 `"$MEMBER_COOKIE"` → `GET /api/ping-tasks/{id}/records` | 返回 200，正常获取探测记录 | — |
| RB-7 | 未登录访问 | 不携带任何 Cookie 或 API Key 直接请求 `GET /api/ping-tasks` | 返回 401 Unauthorized | — |
| RB-8 | Member UI 按钮可见但操作 403 | 使用 `${MEMBER_USER}` / `${MEMBER_PASS}` 登录浏览器 → 访问 `/settings/ping-tasks` | 页面可进入，Add/Delete/Toggle 按钮可见，但点击后 API 返回 403，toast 显示错误提示 | — |

---

## 八、API 接口验证

### 8.1 CRUD 基本流程

```bash
# 创建
curl -s -b "$ADMIN_COOKIE" -X POST http://localhost:9527/api/ping-tasks \
  -H 'Content-Type: application/json' \
  -d '{"name":"Test ICMP","probe_type":"icmp","target":"8.8.8.8","interval":30}'
# → 返回 {"data":{"id":"<uuid>","name":"Test ICMP","probe_type":"icmp",...}}

# 列表
curl -s -b "$ADMIN_COOKIE" http://localhost:9527/api/ping-tasks
# → 返回 {"data":[...]}

# 详情
curl -s -b "$ADMIN_COOKIE" http://localhost:9527/api/ping-tasks/<id>
# → 返回 {"data":{"id":"<id>",...}}

# 更新（仅 API，前端无编辑 UI）
curl -s -b "$ADMIN_COOKIE" -X PUT http://localhost:9527/api/ping-tasks/<id> \
  -H 'Content-Type: application/json' \
  -d '{"name":"Renamed","enabled":false}'
# → 返回 {"data":{"id":"<id>","name":"Renamed","enabled":false,...}}

# 查询记录
curl -s -b "$ADMIN_COOKIE" \
  "http://localhost:9527/api/ping-tasks/<id>/records?from=2026-01-01T00:00:00Z&to=2026-12-31T23:59:59Z"
# → 返回 {"data":[...]}

# 删除
curl -s -b "$ADMIN_COOKIE" -X DELETE http://localhost:9527/api/ping-tasks/<id>
# → 返回 {"data":"ok"}
```

### 8.2 边界与错误场景

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| API-1 | 无效 probe_type | POST 创建，probe_type 设为 "udp" | 返回 422（`VALIDATION_ERROR`），message 含 "probe_type must be icmp, tcp, or http" | — |
| API-2 | 不存在的 task ID | GET `/api/ping-tasks/nonexistent` | 返回 404，message 含 "Ping task nonexistent not found" | — |
| API-3 | 删除不存在的 task | DELETE `/api/ping-tasks/nonexistent` | 返回 404 | — |
| API-4 | 更新不存在的 task | PUT `/api/ping-tasks/nonexistent` | 返回 404 | — |
| API-5 | 更新无效 probe_type | PUT 更新，probe_type 设为 "xyz" | 返回 422 校验错误 | — |
| API-6 | 部分更新 | PUT 只传 `{"name":"NewName"}` | 仅 name 字段更新，其他字段保持不变 | — |
| API-7 | records 缺少时间参数 | GET `/api/ping-tasks/<id>/records`（不带 from/to） | 返回 422（Axum QueryRejection，必填参数缺失） | — |
| API-8 | records 按 server_id 过滤 | GET records 带 `server_id=<sid>` 参数 | 仅返回指定 server 的记录 | — |
| API-9 | 空 server_ids 创建 | POST 创建，不传 server_ids 字段 | 默认为 `[]`（`#[serde(default)]`），代表所有服务器执行 | — |
| API-10 | 默认 interval | POST 创建，不传 interval 字段 | 默认为 60 秒（`default_interval()`） | — |
| API-11 | 默认 enabled | POST 创建，不传 enabled 字段 | 默认为 true（`default_true()`） | — |
| API-12 | records from > to | GET records 带 `from=2026-12-31T00:00:00Z&to=2026-01-01T00:00:00Z` | 返回 200，data 为空数组（不报错） | — |
| API-13 | interval 为 0 | POST 创建，interval 设为 0 | 服务端无校验，创建成功。同步到 Agent 后被 `max(interval, 5)` 钳位为 5 秒 | — |
| API-14 | interval 为负数 | POST 创建，interval 设为 -1 | 服务端无校验，创建成功；但服务端同步时会先将 `i32` 转为 `u32`，负数会变成超大值，不会按 5 秒执行 | — |

---

## 九、Agent 同步

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| SY-1 | 创建后同步 | 创建 ping task → 检查 Agent 日志 | Agent 收到 PingTasksSync 消息，等待一个 interval 周期后开始执行探测（首次 tick 被消耗） | — |
| SY-2 | 更新后同步 | 更新 ping task interval → 检查 Agent 日志 | Agent 收到更新后的 PingTasksSync，按新 interval 执行 | — |
| SY-3 | 删除后同步 | 删除 ping task → 检查 Agent 日志 | Agent 收到 PingTasksSync（不含已删除任务），停止对应探测 | — |
| SY-4 | 禁用后同步 | 禁用 ping task → 检查 Agent 日志 | Agent 收到 PingTasksSync（不含已禁用任务） | — |
| SY-5 | Capability 过滤 | 创建 ICMP 任务，Agent 无 CAP_PING_ICMP 权限 | Agent 不会收到该 ICMP 任务（被 capability 过滤） | — |
| SY-6 | 指定 server 同步 | 创建任务并指定 server_ids 仅含 Server A | 仅 Server A 的 Agent 收到任务，其他 Agent 不受影响 | — |
| SY-7 | Agent 重连同步 | Agent 断开后重新连接 | 重连后自动收到当前所有 enabled 的 ping tasks | — |

---

## 十、国际化 (i18n)

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| I18N-1 | 中文显示 | 切换语言为中文 → 访问 ping-tasks 页面 | 标题显示 "Ping 任务"，按钮和提示信息均为中文 | — |
| I18N-2 | 英文显示 | 切换语言为英文 → 访问 ping-tasks 页面 | 标题显示 "Ping Tasks"，按钮和提示信息均为英文 | — |
| I18N-3 | Probe 类型标签 | 切换语言后查看任务列表 | ICMP Ping / TCP 连接(Connect) / HTTP 请求(Request) 正确显示 | — |
| I18N-4 | Toast 消息 | 创建/删除/启用/禁用任务后的 toast | 中文环境显示如 "Ping 任务已创建"，英文环境显示 "Ping task created" | — |
| I18N-5 | Placeholder 文本 | 切换语言后打开创建表单 | 中文环境 placeholder 如 "如 8.8.8.8 或 google.com"，英文如 "e.g. 8.8.8.8 or google.com" | — |
