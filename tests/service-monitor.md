# 服务监控测试用例

## 前置条件

参照 [README.md](README.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

---

## 一、单元测试（Rust 后端）

### 1.1 Service 层 CRUD（18 个已有用例）

运行：`cargo test -p serverbee-server service_monitor`

| # | 测试函数 | 验证内容 |
|---|---------|---------|
| UT1 | `test_create_and_list` | 创建监控器后 list 返回包含该条目 |
| UT2 | `test_create_invalid_type` | monitor_type 不在合法值中返回错误 |
| UT3 | `test_get` | 按 ID 获取已创建的监控器，字段完整 |
| UT4 | `test_get_not_found` | 不存在的 ID 返回 404 |
| UT5 | `test_update` | 部分更新 name/interval/config_json 生效 |
| UT6 | `test_delete` | 删除后 list 不包含该条目 |
| UT7 | `test_delete_not_found` | 删除不存在的 ID 返回 404 |
| UT8 | `test_list_filter_by_type` | `?type=ssl` 只返回 SSL 类型 |
| UT9 | `test_insert_record_and_get_records` | 插入检查记录后可按 monitor_id 查询 |
| UT10 | `test_get_latest_record` | 返回最新的一条记录 |
| UT11 | `test_list_enabled` | 只返回 enabled=true 的监控器 |
| UT12 | `test_update_check_state` | 更新 last_status/consecutive_failures/last_checked_at |
| UT13 | `test_cleanup_records` | 删除指定天数前的记录，保留新记录 |
| UT14 | `test_delete_cascades_records` | 删除监控器时级联删除所有关联记录 |
| UT15 | `test_update_server_ids_json` | 更新关联服务器 ID 列表 |

> ✅ 15 个 service_monitor 单元测试全部通过

### 1.2 Checker 单元测试

运行：`cargo test -p serverbee-server checker`

| # | 测试函数 | 模块 | 验证内容 |
|---|---------|------|---------|
| UT16-18 | `parse_host_port_*` | ssl.rs | 解析 `host`, `host:port`, `[ipv6]:port` 格式 |
| UT19-21 | `build_resolver_*` | dns.rs | 默认 resolver / 自定义 nameserver / 无效 nameserver |
| UT22-24 | `build_headers_*` | http_keyword.rs | 空 headers / 自定义 headers / 无效 headers |
| UT25-27 | `test_tcp_*` | tcp.rs | 成功连接 / 连接拒绝 / 默认超时 |
| UT28-34 | `parse_*` | whois.rs | 日期格式解析(ISO/Simple/Verbose) / Registrar 解析 / 截断处理 |

> ✅ 21 个 checker 单元测试全部通过

### 1.3 集成测试

运行：`cargo test -p serverbee-server --test integration service_monitor`

| # | 测试函数 | 验证内容 |
|---|---------|---------|
| IT1 | `test_service_monitor_crud_and_check` | 创建 TCP 监控 → 列表验证 → 触发检查 → 查记录 → 删除 → 确认删除 |

> ✅ 集成测试通过

---

## 二、E2E 测试 — 列表页（/settings/service-monitors）

### 2.1 空状态

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| E1 | 空列表显示 | 删除所有监控器后进入列表页 | 显示「No service monitors configured yet.」和「Create your first monitor」按钮 | ✅ |

### 2.2 创建监控器 — 5 种类型

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| E2 | 创建 HTTP Keyword 监控 | Add Monitor → Type: HTTP Keyword → Target: `http://localhost:9527/api/health` → Method: GET → Expected Status: 200 → Create | 列表出现新条目，Type 列显示 `HTTP Keyword` badge，触发检查后状态绿点 | ✅ |
| E3 | 创建 TCP 监控 | Add Monitor → Type: TCP → Target: `localhost:9527` → Timeout: 5 → Create | 列表出现新条目，触发检查后状态为绿色（连接成功） | ✅ |
| E4 | 创建 DNS 监控 | Add Monitor → Type: DNS → Target: `example.com` → Record Type: A → Create | 列表出现新条目，Type 列显示 `DNS` badge | ✅ |
| E5 | 创建 SSL 监控 | Add Monitor → Type: SSL → Target: `example.com` → Warning Days: 14 → Critical Days: 7 → Create | 列表出现新条目 | ✅ |
| E6 | 创建 WHOIS 监控 | Add Monitor → Type: WHOIS → Target: `example.com` → Warning Days: 30 → Critical Days: 7 → Create | 列表出现新条目 | ✅ |

### 2.3 创建表单验证

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| E7 | 名称为空时提交 | Add Monitor → Name 留空 → 点击 Create | 表单不提交或显示验证提示 | ✅ |
| E8 | Target 为空时提交 | Add Monitor → 填名称但 Target 留空 → 点击 Create | 表单不提交或显示验证提示 | ✅ |
| E9 | Type 切换联动 | 在创建对话框中依次选择各 Type | 配置区域根据类型动态变化：SSL/WHOIS 显示天数，DNS 显示记录类型，HTTP 显示 Method/Keyword，TCP 显示 Timeout | ✅ |

### 2.4 列表操作

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| E10 | 手动触发检查 | 列表行点击 Trigger Check (Play 图标) | 状态更新，Last Checked 时间刷新，toast 显示 "Check triggered" | ✅ |
| E11 | 启用/禁用开关 | 切换 Enabled 开关 | 开关状态立即更新，禁用后后台不再自动检查 | ✅ |
| E12 | 编辑监控器 | 点击 Edit (Pencil 图标) → 修改名称 → Save Changes | 对话框关闭，列表中名称已更新，toast 显示 "Monitor updated" | ✅ |
| E13 | 删除监控器 | 点击 Delete (Trash 图标) → 确认删除 | 条目从列表消失，toast 显示 "Monitor deleted" | ✅ |
| E14 | 状态徽章颜色 | 查看列表 Status 列 | 成功=绿点，失败=红点，未检查=灰点 | ✅ |

---

## 三、E2E 测试 — 详情页（/service-monitors/:id）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| E15 | 进入详情页 | 点击列表行的 View (Eye 图标) | 显示监控器名称、Type Badge、状态 Badge、目标地址 | ✅ |
| E16 | 统计卡片 | 查看详情页顶部 | 显示 3 张卡片：Uptime%、Avg Latency (ms)、Last Check 时间 | ✅ |
| E17 | 响应时间图表 | 查看详情页图表区域 | 显示 AreaChart，横轴为时间，纵轴为延迟毫秒 | ✅ |
| E18 | Check Now 按钮 | 点击 Check Now | 按钮显示旋转图标，检查完成后统计和记录刷新，toast 显示 "Check triggered" | ✅ |
| E19 | 检查历史表格 | 查看详情页下方 | 显示历史记录表：Time / Status / Latency / Error 列 | ✅ |
| E20 | 返回列表 | 点击面包屑「Back to Service Monitors」 | 导航回列表页 | ✅ |

### 3.1 各类型详情展示

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| E21 | SSL 详情卡片 | 查看 SSL 监控详情 | 显示 Subject、Issuer、Not Before/After、Days Remaining、SHA-256 Fingerprint | ✅ |
| E22 | DNS 详情卡片 | 查看 DNS 监控详情 | 显示 Record Type、Nameserver、Changed、Resolved Values | ✅ |
| E23 | HTTP 详情卡片 | 查看 HTTP 监控详情 | 显示 Status Code、Keyword Found、Response Time | ✅ |
| E24 | TCP 详情卡片 | 查看 TCP 监控详情 | 显示 Connected: Yes/No | ✅ |
| E25 | WHOIS 详情卡片 | 查看 WHOIS 监控详情 | 显示 Registrar、Expiry Date、Days Remaining | ✅ |

---

## 四、API 端点直接测试

用 curl 或 Swagger UI (`/swagger-ui/`) 验证：

### 4.1 读端点（所有已认证用户）

| # | 端点 | 验证内容 |
|---|------|---------|
| A1 | `GET /api/service-monitors` | 返回数组，每项包含完整字段 |
| A2 | `GET /api/service-monitors?type=ssl` | 只返回 monitor_type=ssl 的条目 |
| A3 | `GET /api/service-monitors/{id}` | 返回 monitor + latest_record 联合对象 |
| A4 | `GET /api/service-monitors/{id}/records?limit=10` | 返回最多 10 条记录，按时间降序 |
| A5 | `GET /api/service-monitors/{id}/records?from=...&to=...` | 返回指定时间范围内的记录 |

> ✅ A1, A3, A4 测试通过

### 4.2 写端点（仅 Admin）

| # | 端点 | 验证内容 |
|---|------|---------|
| A6 | `POST /api/service-monitors` | 创建成功返回完整对象，ID 为 UUID |
| A7 | `PUT /api/service-monitors/{id}` | 部分更新，只修改传入的字段 |
| A8 | `DELETE /api/service-monitors/{id}` | 删除成功，关联记录级联清除 |
| A9 | `POST /api/service-monitors/{id}/check` | 立即执行检查，返回 ServiceMonitorRecord |
| A10 | `POST /api/service-monitors` (invalid type) | 返回 400，提示类型无效 |

> ✅ A6, A8, A9, A10 测试通过

### 4.3 权限测试

| # | 端点 | 验证内容 |
|---|------|---------|
| A11 | `GET /api/service-monitors` (未登录) | 返回 401 |
| A12 | `POST /api/service-monitors` (Member 用户) | 返回 403（仅 Admin 可写） |
| A13 | `DELETE /api/service-monitors/{id}` (Member 用户) | 返回 403 |

> ✅ A11 测试通过（401 confirmed）

---

## 五、后台调度与告警

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| S1 | 自动调度执行 | 创建 interval=60 的监控器 → 等待 70 秒 | 历史记录出现自动检查条目 | ✅ |
| S2 | 连续失败告警 | 创建 TCP 监控目标不可达 + retry_count=2 + 通知组 | 连续失败 3 次后触发通知 | — |
| S3 | 恢复通知 | 将 S2 的目标修改为可达地址 | 下次检查成功后触发恢复通知 | — |
| S4 | Maintenance 跳过通知 | 关联服务器设为 maintenance → 检查失败 | 不发送告警通知 | — |
| S5 | 禁用不调度 | 禁用监控器 → 等待超过 interval | 无新的检查记录产生 | ✅ |
| S6 | 记录清理 | 配置 retention 天数 → 运行 cleanup | 超期记录被删除，近期保留 | ✅ |

---

## 六、i18n 国际化

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| I1 | 中文界面 | 切换中文 → 进入服务监控页 | 标题「服务监控」、按钮「添加监控」、表头「名称/类型/目标/间隔/状态」均为中文 | ✅ |
| I2 | 英文界面 | 切换英文 → 进入服务监控页 | 标题 "Service Monitors"、按钮 "Add Monitor"、表头 "Name/Type/Target/Interval/Status" 均为英文 | ✅ |
| I3 | 监控类型标签 | 查看 Type 列和创建表单 Type 下拉 | 5 种类型（SSL/DNS/HTTP/TCP/WHOIS）在两种语言下正确显示 | ✅ |

---

## 七、响应式与可访问性

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| R1 | 移动端列表 | 窗口宽度 < 768px 查看列表 | 表格可横向滚动或堆叠显示，操作按钮可点击 | ✅ |
| R2 | 移动端详情 | 窗口宽度 < 768px 查看详情 | 统计卡片单列排列，图表自适应宽度 | ✅ |
| R3 | 创建对话框移动端 | 窗口宽度 < 768px 打开创建表单 | 对话框全屏或适当缩放，字段可正常输入 | ✅ |
