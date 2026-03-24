# Docker 容器监控测试用例

## 前置条件

参照 [TESTING.md](../TESTING.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

Agent 所在主机需要安装 Docker 环境。

---

## 一、能力控制

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| D1 | 能力开关（启用） | Server Detail → 启用 Docker Management capability | Docker 按钮出现 | ✅ |
| D2 | 能力开关（禁用） | 关闭 CAP_DOCKER | Docker 按钮消失，API 返回 403 | ✅ |

---

## 二、Docker 页面（/servers/:serverId/docker）

### 2.1 空状态与不可用

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| D3 | Docker 不可用 | Agent 无 Docker 环境 | 页面显示 "Docker is not available" 占位 | ✅ |
| D4 | Docker 可用无容器 | Agent 有 Docker 但无容器 | 显示概览卡片 + "No containers found" | ✅ |

### 2.2 概览卡片

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| D5 | 概览卡片 | 查看页面顶部 | 显示 5 张卡片：Running / Stopped / Total CPU / Total Memory / Docker Version | ✅ |

### 2.3 容器列表

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| D6 | 容器列表渲染 | 查看容器表格 | 显示 Name / Image / Status / CPU% / Memory / Network I/O | ✅ |
| D7 | 容器搜索 | 输入容器名或镜像名 | 表格过滤匹配项 | ✅ |
| D8 | 容器过滤 | 点击 Running / Stopped / All 按钮 | 切换过滤状态 | ✅ |

### 2.4 容器详情弹窗

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| D9 | 容器详情弹窗 | 点击容器行 | Dialog 显示元信息 + Stats + Logs | ✅ |
| D10 | 容器 Stats | 查看详情弹窗 | 4 张迷你卡片：CPU / Memory（含进度条） / Net I/O / Block I/O | ✅ |
| D11 | 容器日志流 | 查看详情弹窗日志区域 | 自动连接，显示实时日志流 | ✅ |
| D12 | 日志 Follow | 开启 Follow → 关闭 Follow | 开启时新日志自动滚动到底部，关闭后停止滚动 | ✅ |
| D13 | 日志 stderr 颜色 | 查看 stderr 日志行 | 显示红色文本 | ✅ |
| D14 | 日志清除 | 点击 Clear | 日志区域清空 | ✅ |
| D15 | 日志连接状态 | 查看连接状态指示器 | 连接时绿色圆点 + "Connected"，断开时灰色 + "Disconnected" | ✅ |

### 2.5 实时数据

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| D16 | 实时数据更新 | WS 推送 docker_update | 容器列表和 Stats 实时刷新 | ✅ |
| D21 | 订阅/退订 | 进入/离开 Docker 页 | 进入时 WS 发送 docker_subscribe，离开时发送 docker_unsubscribe | ✅ |
| D22 | docker_availability_changed | Agent Docker daemon 停止 → 恢复 | 页面切换为不可用占位 → 自动恢复 | — |

### 2.6 事件与网络/卷

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| D17 | 事件时间线 | 查看 Docker 事件 | start/stop/die 等按时间倒序显示，相对时间戳 | ✅ |
| D18 | 事件 Badge | 查看事件类型 Badge | container/image/network/volume 各有不同样式 | ✅ |
| D19 | 网络列表弹窗 | 点击 Networks 按钮 | Dialog 显示网络 Name / Driver / Scope / 容器数 | ✅ |
| D20 | 卷列表弹窗 | 点击 Volumes 按钮 | Dialog 显示卷 Name / Driver / Mountpoint / 创建时间 | ✅ |

---

## 三、i18n

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| D23 | i18n 中文 | 切换中文 | Docker 按钮显示 "Docker"，能力名显示 "Docker 管理" | ✅ |
| D24 | i18n 英文 | 切换英文 | Docker 按钮显示 "Docker"，能力名显示 "Docker Management" | ✅ |
