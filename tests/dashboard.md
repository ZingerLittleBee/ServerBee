# 自定义仪表盘测试用例

## 前置条件

参照 [README.md](README.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

---

## 一、仪表盘 CRUD

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| DB1 | 默认仪表盘自动创建 | 首次登录 → `/` 自动创建默认 Dashboard | 显示 6 个预设 widget（5 stat-number + 1 server-cards） | ✅ |
| DB2 | 默认仪表盘幂等 | 刷新页面 | 仍显示相同仪表盘 ID 和 6 个 widget | ✅ |
| DB3 | 新建仪表盘 | 点击 `+ New` → 输入名称 → 创建 | 切换到新仪表盘（空白） | ✅ |
| DB4 | 切换仪表盘 | 下拉选择另一个仪表盘 | 页面加载对应 widget 布局 | ✅ |
| DB5 | 设为默认 | 选择非默认仪表盘 → 点击 Set Default（星号按钮） | 刷新后默认加载该仪表盘 | ✅ |
| DB6 | 删除仪表盘 | 选择非默认仪表盘 → 点击 Delete → 确认 | 切换回其他仪表盘 | ✅ |
| DB7 | 删除默认仪表盘保护 | 查看默认仪表盘 | Delete 按钮不显示或禁用 | ✅ |
| DB8 | RBAC — Member 只读 | Member 用户登录 | 不显示 Edit/New/Delete 按钮，仅可查看 | — |

---

## 二、编辑模式

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| DB9 | 进入编辑模式 | 点击 Edit 按钮 | widget 可拖拽/调整大小，显示 Save/Cancel 按钮 | ✅ |
| DB10 | 拖拽布局 | 编辑模式下拖拽 widget 到新位置 | 释放后布局更新 | — |
| DB11 | 调整大小 | 编辑模式下拖拽 widget 右下角 | 尺寸变化 | — |
| DB12 | 添加 Widget | 编辑模式 → Add Widget → Widget Picker → 选择类型 → 配置 → 确认 | widget 出现在画布 | ✅ |
| DB13 | 编辑 Widget 配置 | 编辑模式 → hover widget → 点击铅笔图标 → 修改 → 确认 | widget 更新 | ✅ |
| DB14 | 删除 Widget | 编辑模式 → hover widget → 点击垃圾桶图标 | widget 移除 | ✅ |
| DB15 | 保存布局 | 编辑后点击 Save | PUT /api/dashboards/:id → 刷新后布局保持 | ✅ |
| DB16 | 取消编辑 | 编辑后点击 Cancel | 所有修改丢弃，恢复原布局 | ✅ |

---

## 三、Widget 类型渲染

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| DB17 | stat-number | 添加 stat-number widget → 选择 metric | 显示图标 + 数值 + 标签 | ✅ |
| DB18 | server-cards | 添加 server-cards widget | 显示服务器卡片网格，实时更新 | ✅ |
| DB19 | gauge | 添加 gauge widget → 选择服务器 + 指标 | 显示径向进度条 + 百分比 | — |
| DB20 | top-n | 添加 top-n widget → 选择指标 + 数量 | 显示排名列表 + 进度条 | — |
| DB21 | line-chart | 添加 line-chart widget → 选择服务器 + 指标 + 时间范围 | 显示历史折线图 | — |
| DB22 | multi-line | 添加 multi-line widget → 选择多台服务器 + 指标 | 显示多线对比图 + 图例 | — |
| DB23 | traffic-bar | 添加 traffic-bar widget → 可选服务器 | 显示 in/out 堆叠柱状图 | — |
| DB24 | disk-io | 添加 disk-io widget → 选择服务器 | 显示磁盘读写折线图 | — |
| DB25 | alert-list | 添加 alert-list widget | 显示告警事件列表（红/绿状态点 + 规则名 + 服务器 + 相对时间） | — |
| DB26 | service-status | 添加 service-status widget | 显示服务监控点阵（绿/黄/红/灰圆点），hover 显示监控名 + 状态 | — |
| DB27 | server-map (无 GeoIP) | 添加 server-map widget，未安装 GeoIP | 显示 "GeoIP database not installed" + Download 按钮（admin）/ 仅文字（member） | — |
| DB27a | server-map (GeoIP 下载) | 点击 Download GeoIP Database 按钮 | loading → 成功 toast → 地图开始显示数据 | — |
| DB27b | server-map (有数据) | GeoIP 已安装 + Agent 有公网 IP | SVG 世界地图高亮有服务器的国家，圆形标记在国家质心，底部显示 "GeoIP by DB-IP" | — |
| DB28 | markdown | 添加 markdown widget → 输入 Markdown 内容 | 渲染标题/粗体/链接/列表，无 XSS（`<script>` 被转义） | — |

---

## 四、响应式 & 移动端

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| DB29 | 移动端布局 | 窗口宽度 < 768px | widget 按 sort_order 垂直排列（非 grid 布局） | ✅ |
| DB30 | 桌面端布局 | 窗口宽度 ≥ 768px | 12 列 grid 布局 + 拖拽调整 | ✅ |

---

## 五、API 验证

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| DB31 | GET /api/dashboards | 请求仪表盘列表 | 返回所有仪表盘，按 sort_order 排序 | ✅ |
| DB32 | GET /api/dashboards/default | 首次调用 | 自动创建 → 返回 6 个 widget → 第二次返回相同 ID | ✅ |
| DB33 | POST /api/dashboards | 创建新仪表盘 | 返回 dashboard model | ✅ |
| DB34 | PUT /api/dashboards/:id | 更新名称/widget diff（增/改/删） | 返回完整 DashboardWithWidgets | ✅ |
| DB35 | DELETE /api/dashboards/:id | 删除非默认仪表盘 → 200 → 删除默认仪表盘 → 400 | ✅ |
| DB36 | GET /api/alert-events | 请求告警事件 | 返回聚合告警事件列表，firing 在前，支持 limit 参数 | ✅ |
| DB37 | OpenAPI | 访问 `/swagger-ui/` | 包含 6 个 dashboards + 1 个 alert-events 端点 | ✅ |

---

## 六、i18n

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| DB38 | 中文模式 | 切换中文 | 编辑/保存/取消/添加组件/新建仪表盘/删除等按钮显示中文 | ✅ |
| DB39 | 英文模式 | 切换英文 | Edit/Save/Cancel/Add Widget/New Dashboard/Delete 显示英文 | ✅ |
| DB40 | Widget Picker 中文 | Widget 选择面板 | 12 种类型的名称和描述显示中文 | ❌ |
