# iOS App 端到端冒烟测试

> 目标读者：QA / 维护者，需要在 TestFlight 发布前对 iOS app 做完整 UI 端到端验证。
>
> 范围：覆盖 2026-05-20 七个 plan（1～7）的所有用户可见行为。API 层用例见 [`mobile-ios.md`](mobile-ios.md)。

---

## 前置条件

### 硬件 / 软件
- Mac + Xcode 26.5（含 iOS 26.4 模拟器 runtime）
- **真机 iPhone 一台**（推送、相机部分必须真机；模拟器跑不了 APNs）
- 同一局域网内的本地 Server，或可达的远端 Server

### 服务端
按 [`README.md`](README.md) 启动 Server + Agent。最小配置示例：

```bash
SERVERBEE_ADMIN__PASSWORD=admin123 \
SERVERBEE_AUTH__SECURE_COOKIE=false \
cargo run -p serverbee-server &

SERVERBEE_SERVER_URL="http://127.0.0.1:9527" \
SERVERBEE_ENROLLMENT_CODE="<code>" \
cargo run -p serverbee-agent &
```

记下 Server URL（本机 IP，例如 `http://192.168.1.100:9527`），iOS 测试用。

### iOS App 构建

```bash
cd apps/ios
xcodegen generate
xcodebuild build -scheme ServerBee \
  -destination 'platform=iOS Simulator,name=iPhone 17,OS=26.4'
```

真机构建：用 Xcode GUI 选择真机 destination + 自己的开发者证书。

### 准备一组 Mobile 凭据

提前从 Web 端登录 admin → 设置 → Mobile 配对码（或调 API），拿到一个 8 位 enrollment code，用来测 QR 配对。也保留 `admin / admin123` 用于用户名密码登录。

---

## 一、构建 / 首启

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| B1 | 模拟器干净安装 | `xcrun simctl erase booted` 后 `xcodebuild ... -destination ... | xcrun simctl install booted ...` 启动 | App 出现登录页，无崩溃 | ☐ |
| B2 | 真机首启 | Xcode Run 到真机 | App 启动 ≤ 3s，登录页正常 | ☐ |
| B3 | Light / Dark 切换 | 启动后系统 Settings 切外观，回 App | 颜色全套切换，无残留 light/dark 色块 | ☐ |
| B4 | Dynamic Type 极大 | 系统 Settings → 显示与文字大小 → 拉到 5/5 | 各页面无文字截断 / 重叠 | ☐ |
| B5 | SwiftLint clean build | `xcodebuild build` 输出 | 0 SwiftLint warning，0 compiler warning | ☐ |

---

## 二、鉴权

### 2.1 用户名密码登录

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| A1 | 正常登录 | Server URL 输 `http://192.168.1.x:9527` → 用户名 admin / 密码 admin123 → Sign In | 进入 Servers tab，~1s 出现服务器卡片 | ☐ |
| A2 | http:// 警告条 | 同上输入 `http://` 开头 URL | 顶部出现黄色 `InsecureURLBanner`「Insecure connection」 | ☐ |
| A3 | https:// 无警告 | URL 改成 `https://...` | 无警告条 | ☐ |
| A4 | 错误密码 | 密码改成 `wrong` | 红色错误提示，停留登录页 | ☐ |
| A5 | 限流 | 连续 20 次错误密码 | 红色 "Too many attempts, try later" | ☐ |
| A6 | URL 无效 | URL 输 `notaurl` | 友好错误提示，无崩溃 | ☐ |
| A7 | 网络不可达 | URL 输 `http://10.0.0.99:9527`（无主机） | 友好错误，无白屏卡死 | ☐ |
| A8 | TOTP 流程 | 服务端给 admin 启 2FA，登录 | 进入 TOTP 输入页 | ☐ |
| A9 | TOTP 键盘避让 | 在 iPhone SE 模拟器跑 A8 | TOTP 输入框始终在键盘上方可见 | ☐ |
| A10 | TOTP 错误 | 输错 6 位码 | 红色提示，停留 TOTP 页 | ☐ |

### 2.2 QR 扫码配对

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| Q1 | 首次扫码（未授权相机） | LoginView → Scan QR → 第一次弹出系统权限弹窗 → 允许 | 相机开启，能扫码 | ☐ |
| Q2 | 拒绝相机权限 | 设置 → Privacy → Camera 关掉 ServerBee → 回 App 扫码 | 出现 "Camera Access Denied" 屏 + `Open Settings` 按钮，点击跳系统设置 | ☐ |
| Q3 | 重新授权后能扫 | 设置里重开相机 → 回 App 扫码 | 相机正常工作 | ☐ |
| Q4 | 扫到合法 code | Web 端生成 code → 用真机扫 | 自动登录，进入 Servers tab | ☐ |
| Q5 | 扫到过期 code | 等 code 过期（默认 10 min） | 友好错误，停留扫描页 | ☐ |
| Q6 | 扫码界面后台返回 | 扫描中按 Home → 5s 后回 App | 相机自动恢复，无卡死 | ☐ |

---

## 三、实时数据 / WebSocket

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| W1 | WS 首次连接 | 登录后看 Servers tab | 卡片 CPU/MEM/网速实时跳动 | ☐ |
| W2 | 后台 5min 再前台 | 进 App → Home → 等 5 分钟 → 回 App | 数据 ≤ 2s 内重新跳动，无显示「连接已断」 | ☐ |
| W3 | 后台 30min 再前台 | 同上但 30 分钟 | 数据恢复跳动 ≤ 5s | ☐ |
| W4 | 杀掉 Server 端 | `pkill serverbee-server` | App 顶部出现网络断/离线提示，数据停止跳动 | ☐ |
| W5 | 重启 Server | 重新 `cargo run -p serverbee-server` | App 自动重连，数据恢复（无需手动操作） | ☐ |
| W6 | 飞行模式切换 | 开飞行模式 → 10s → 关 | 出现 OfflineBanner → 关后 banner 消失，WS 自恢复 | ☐ |
| W7 | Wi-Fi 切流量 | 切到蜂窝网络 | App 不丢登录态，WS 重连 | ☐ |
| W8 | 杀 App 重启 | 上滑杀 → 重开 | 自动凭存储的 token 进入主界面 | ☐ |
| W9 | Tab 切换不断 WS | Servers → Alerts → Settings → Servers | WS 始终保持连接（关键回归点 — 修复前 `.onDisappear` 会误关） | ☐ |
| W10 | 多 Agent 并发 | 启第 2 个 Agent | 两张卡片都实时跳，无丢推送 | ☐ |

---

## 四、服务器列表 / 详情

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| S1 | 列表排序 | 多个服务器，部分 offline | online 在上，offline 在下 | ☐ |
| S2 | 搜索 debounce | 搜索框快速打字「ser ver」 | 不会逐字符触发筛选，停顿 250ms 后才过滤 | ☐ |
| S3 | 搜索无结果 | 输入不匹配关键词 | "No matching servers" 友好空态 | ☐ |
| S4 | 拉刷 | 列表下拉 | 出现刷新图标，1s 内完成 | ☐ |
| S5 | 错误重试 | 杀 Server → 拉刷 | 显示错误信息 + Try again 按钮 | ☐ |
| S6 | 点击卡片进详情 | 任意卡片 | 进入 ServerDetailView，全部 metric 卡显示 | ☐ |
| S7 | 历史曲线 | Detail 页底部 History 按钮 | 进入 MetricsHistoryView，曲线渲染 | ☐ |
| S8 | 时间范围切换 | History 页切 1h / 6h / 24h | 曲线刷新，无白屏 | ☐ |
| S9 | Detail 返回不断 WS | Detail → 返回 → 再进 | WS 不断流；卡片数据连续 | ☐ |
| S10 | Detail offline 状态 | 杀掉对应 agent | Detail 显示 "Offline since ..." | ☐ |

---

## 五、告警

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| AL1 | 列表加载 | Alerts tab | 列出所有历史 alert event | ☐ |
| AL2 | 实时 firing | Web 端造一条 CPU 阈值，让 agent 触发 | App 列表 ≤ 2s 内出现新事件（无需手动刷新） | ☐ |
| AL3 | resolved 实时 | 触发后让 CPU 回落 | 同一事件 status 由 firing → resolved（不重复显示两行） | ☐ |
| AL4 | 突发多条 | 同时触发 5 条规则 | 列表正确显示 5 条；后端 5 次 fetch 不应并发风暴（用 Charles/网络面板检查） | ☐ |
| AL5 | 错误重试 | 杀 Server → Alerts 拉刷 | 错误 + Try again 按钮 | ☐ |
| AL6 | 详情页 | 点击 alert event | 进入 AlertDetailView，显示规则、阈值、历史 | ☐ |
| AL7 | View Server 跳转 | 详情页 "View Server" 按钮 | 跳 ServerDetailView，对应正确服务器 | ☐ |

> **⚠️ 已知阻塞 B1**：alert 推送 deep link 当前进入占位页而非真页面（见跨 plan review）。AL2/AL3 通过 WS 无影响；只在 §六 推送场景出现。

---

## 六、推送通知（真机必需）

### 6.1 注册

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| P1 | 首次允许通知 | 登录 → 系统弹出推送授权 → 允许 | 后端 device_tokens 表新增一行 | ☐ |
| P2 | 拒绝通知 | 同上但选拒绝 | App 正常运行，无推送（后端无 device_token） | ☐ |
| P3 | 再次允许 | 设置 → ServerBee → 通知 → 允许 | App 重新注册 device token | ☐ |

### 6.2 接收

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| P4 | 前台推送 | App 在前台 → Web 触发告警 | 出现系统横幅或 inApp banner | ☐ |
| P5 | 后台推送 | App 在后台 → 触发告警 | 锁屏 / 通知中心出现通知 | ☐ |
| P6 | 杀进程推送 | 上滑杀 App → 触发告警 | 通知中心收到推送 | ☐ |

### 6.3 点击 deep link

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| P7 | 后台点推送 | 锁屏点告警通知 | App 打开 → 直接进入对应 ServerDetail 或 AlertDetail（**B1 验证点**） | ☐ |
| P8 | 杀进程点推送 | 杀 App → 点通知 | 同 P7（冷启动 deep link） | ☐ |
| P9 | server_id deep link | 后端 push payload 含 `server_id` | App 跳到对应 ServerDetail | ☐ |
| P10 | rule_id deep link | payload 含 `rule_id` | App 跳到对应 AlertDetail（**当前 B1 — 进占位页**） | ☐ |

### 6.4 登出 / 切账

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| P11 | 登出后无推送 | 登出 → Web 触发告警 | **App 不再收到推送**（验证 #10 修复）；后端 device_tokens 表对应行已删 | ☐ |
| P12 | 切账户隔离 | A 登出 → B 在同台机登录 → 触发 A 的告警 | B 不收到（**B2 验证点**：登出还需关 WS） | ☐ |

---

## 七、设置

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| SET1 | Theme 切换 | Settings → Theme → Dark/Light/System | App 立即切换 | ☐ |
| SET2 | Device Name 编辑 | Settings → Device Name → 编辑 → 保存 | 后端设备列表更新；下次登录沿用 | ☐ |
| SET3 | Device Name 持久 | 杀 App 重启 | 自定义 device name 保留 | ☐ |
| SET4 | Server URL 切换 | Settings → Server URL（如有）改成 http:// | 警告条出现 | ☐ |
| SET5 | About / Version | Settings → About | 显示版本号、构建时间 | ☐ |
| SET6 | 登出 | Settings → Sign Out | 回到 LoginView；token 已清；**WS 已关闭**（**B2 验证点**） | ☐ |
| SET7 | 语言切换 | 系统 Settings → ServerBee → Language → 简体中文 → 回 App | 全 App 切中文 | ☐ |
| SET8 | App 内无语言 Picker | Settings → Appearance | 只有 Theme，没有 Language（验证 Plan 4 决策） | ☐ |

---

## 八、可访问性

### 8.1 VoiceOver

启用：系统 Settings → 辅助功能 → VoiceOver → 开。

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| V1 | 服务器卡片 | 在卡片上左右滑 | 一次性念出："server-name, online, CPU 45 percent, Memory 60 percent, Disk 30 percent"，不分别念图标 | ☐ |
| V2 | offline 卡片 | offline 卡片 | "server-name, offline, last seen 5 minutes ago" | ☐ |
| V3 | 状态徽章 | Detail 页顶部 badge | 一次念出 "online / offline" 不重复念圆点 | ☐ |
| V4 | 告警卡 | Alerts tab swipe | "CPU usage on server-x, firing since 10:23"，一次念完 | ☐ |
| V5 | Offline banner | 飞行模式打开 | 念 "Offline. Trying to reconnect." | ☐ |
| V6 | InsecureURLBanner | LoginView 输 http:// | 念 "Warning. Insecure connection over HTTP." | ☐ |
| V7 | Retry button | 错误态 | 念 "Try again, button" 并可激活 | ☐ |
| V8 | 历史曲线 | MetricsHistoryView | 至少念出标题；曲线本身可不被读 | ☐ |

### 8.2 Dynamic Type

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| D1 | 最大文字大小 | Settings → 显示 → 拉到「AX5」 | Servers 卡片不溢出、不截断 | ☐ |
| D2 | 最小文字大小 | 拉到最左 | UI 仍合理（不空旷） | ☐ |
| D3 | Detail 大图标 | ServerDetailView | 60pt 图标随字号略放大 | ☐ |

### 8.3 Dark Mode

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| DM1 | 在线绿点 | online 卡片 | Dark 下偏亮绿色（#34D399），不刺眼 | ☐ |
| DM2 | 离线红点 | offline 卡片 | Dark 下偏亮红色（#F87171） | ☐ |
| DM3 | InsecureURLBanner | http:// 登录页切 dark | 黄色文字可读，对比度 ≥ 4.5:1 | ☐ |
| DM4 | Charts | History 页切 dark | 曲线颜色与背景对比清晰 | ☐ |

---

## 九、本地化

| # | 场景 | 步骤 | 期望 | 状态 |
|---|------|------|------|------|
| L1 | 英文页面 | 系统 ServerBee → Language → English | 所有界面英文 | ☐ |
| L2 | 中文页面 | Language → 简体中文 | 所有界面中文（含 toast、错误） | ☐ |
| L3 | RelativeTime 中文 | 中文环境下看 "5 分钟前" 等 | 不应出现 "5m ago" | ☐ |
| L4 | ByteCountFormatter | 中文环境 MEM 显示 | 显示中文单位 "字节 / 千字节" 或保留 KB/MB（任一都可，注意一致性） | ☐ |
| L5 | xcstrings 无漏 | 切中文翻所有页面 | 没有英文残留 | ☐ |
| L6 | ISO 时间解析 | 服务端返回不含小数秒的 RFC3339 | App 「N 分钟前」正常显示（**Plan 5 #32 验证**） | ☐ |

---

## 十、已知阻塞复测

跨 plan review 在 §六 / §七 中标了三处。整理在这里方便回归：

| # | Blocker | 复现 | 期望（修复后） | 状态 |
|---|---------|------|----------------|------|
| BL-1 | Alert deep link 走占位 | P10：推送含 `rule_id` → 点击 | 进入真 AlertDetailView，**不**是 ContentUnavailableView | ☐ |
| BL-2 | 登出不关 WS | P12 / SET6：登出后看 Server 后台日志 | 不再有该设备的 WS 重连请求 | ☐ |
| BL-3 | Alert 突发并发 fetch | AL4：触发 5 条规则同时 | 后端 `/api/alert-events` 命中次数 = 1（debounced），不是 5 | ☐ |

---

## 十一、回归（保证 plan 旧功能没被打破）

| # | 场景 | 期望 | 状态 |
|---|------|------|------|
| R1 | Token 自动 refresh | access token 15min 过期后下次请求 | 静默 refresh，用户无感 | ☐ |
| R2 | refresh 失败网络抖动 | 网络瞬断 + refresh 请求超时 | **不踢出登录**（Plan 2 #15） | ☐ |
| R3 | refresh 401 | 后端撤销 refresh token | 踢回 LoginView，提示 session expired | ☐ |
| R4 | 冷启动 cold start | Token 有效情况下杀 App 重启 | Servers / Alerts tab 立即有内容，不空白（Plan 2 #7） | ☐ |
| R5 | 同卡 multi tap | 快速点击同一卡片 5 次 | 不重复 push 同一详情，不卡死 | ☐ |
| R6 | 中文 push 内容 | 服务端发中文标题告警 | 推送中文正常显示 | ☐ |

---

## 通过标准

**Phase A（TestFlight 准入）**：B1-B5 + A1-A10 + Q1-Q6 + W1-W10 + S1-S10 + AL1-AL7 + R1-R6 全部 ✅。
**Phase B（App Store 准入）**：全部 ✅ 含 P1-P12 + SET1-SET8 + L1-L6 + BL-1/2/3 已修。
**Phase C（产品就绪）**：全部 ✅ 含 V1-V8 + D1-D3 + DM1-DM4。

## 报告模板

完成后写一份 `tests/results/ios-smoke-YYYY-MM-DD.md`：

```markdown
# iOS Smoke Test — YYYY-MM-DD

- Builder: <name>
- Build: <commit sha>
- Device: <real iPhone model + iOS version>
- Simulator: iPhone 17 / iOS 26.4

## 结果

| 章节 | 通过 / 总数 |
|------|------------|
| 构建 | 5/5 |
| 鉴权 | 14/16 |
| ... | |

## 失败用例

- A8 TOTP：...
- BL-1：...

## 截图 / 录屏
（附 Photos / Slack 链接）
```
