# 安全事件（SSH / 端口扫描）测试用例

## 前置条件

参照 [README.md](README.md) 中的「启动本地环境」部分完成 Server + Agent 启动和登录。

> **平台限制**：本功能 Linux only。macOS/Windows 上的 agent 启动时会自动跳过 `SecurityManager`，无需额外配置。

> **能力位**：新增 `CAP_SECURITY_EVENTS`（bit 8 = 256），默认开（含在 `CAP_DEFAULT=316` 内）。已存在的 server 行若 `capabilities=60`（旧默认）会被自动 `OR 256`；自定义掩码不动。

> **端口扫描可选项**：`security.port_scan.enabled` 默认 **关**。启用扫描检测需要：
> 1. agent.toml: `[security.port_scan] enabled = true`
> 2. systemd unit 添加 `AmbientCapabilities=CAP_NET_RAW CAP_NET_ADMIN`
> 3. 系统装 `conntrack-tools`：`apt install conntrack` / `yum install conntrack-tools`
> 4. `systemctl daemon-reload && systemctl restart serverbee-agent`

---

## 一、SSH 检测（默认开，无需额外配置）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| S1 | Agent 启动检测 | 启动 agent，看日志 | 启动正常（SecurityManager 设计为静默成功，仅在禁用时日志）| ✅ (VPS) |
| S2 | 成功登录 → first_seen=true | 用从未用过的 (user, IP) 组合 SSH 登录目标 VPS | 90s 内 `GET /api/security/events?event_type=ssh_login` 看到一条 `first_seen=true` 的记录 | — |
| S3 | 成功登录 → first_seen=false | 用同一 (user, IP) 再次 SSH | 新事件 `first_seen=false` | — |
| S4 | 单用户 hammering | `for i in {1..15}; do sshpass -p wrong ssh root@vps true; done` | 触发，`severity=medium`（distinct_users=1）| ✅ (VPS, 3 events) |
| S5 | 多用户 credential stuffing | 失败时轮换 user（root/admin/postgres/git/nginx）| `severity=high` 或 `critical`（distinct_users ≥ 2 / ≥ 5）| — |
| S6 | 窗口外不触发 | 5 次失败 → 等 70s → 再 5 次失败 | 不触发（滑动窗口已过）| — |
| S7 | 触发后窗口内不重复 | 12 次失败触发后立即再来 5 次 | 不再触发；窗口外重置后才能再触发 | — |
| S8 | invalid_user 标记 | SSH 用不存在的用户 `nosuchuser` 失败 | evidence 里 `invalid_user_count > 0` | ✅ (VPS, invalid_user_count=10) |
| S9 | IPv6 来源解析 | 从 IPv6 客户端发起失败登录 | source_ip 字段是完整展开的 IPv6 | — |

## 二、端口扫描检测（opt-in，需要 CAP_NET_ADMIN + conntrack-tools）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| P1 | 未启用 → 无事件 | 默认配置下 `nmap -p 1-1000 vps` | 不产生 port_scan 事件（功能默认关）| — |
| P2 | 启用后正常触发 | 配置 + 重启 agent → `nmap -p 1-100 vps` | 60s 内产生 `port_scan` 事件，evidence 含 `distinct_ports` 和 `sample_ports` | — |
| P3 | 同端口反复连接不触发 | `for i in {1..100}; do nc -z vps 80; done` | 不触发（只有 1 个 distinct port）| — |
| P4 | 防火墙日志补充 blocked_count | 配置 ufw `LOG` + nmap | evidence 的 `blocked_count > 0` | — |
| P5 | 缺 conntrack-tools 优雅降级 | 卸载 conntrack-tools，重启 agent | agent 日志报告"scan disabled"，SSH 检测照常运行 | — |
| P6 | 缺 CAP_NET_ADMIN 优雅降级 | 移除 `AmbientCapabilities=CAP_NET_ADMIN`，重启 agent | 同 P5 — 仅 scan 禁用，SSH 不受影响 | — |

## 三、告警规则集成

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| A1 | `ssh_brute_force_detected` 触发 | settings/alerts 用 preset 卡创建规则 → 触发 S4 | 通知组收到通知 | — |
| A2 | dedupe 窗口生效 | 同 IP 连续 2 次触发（间隔 < dedupe_window）| 仅一次通知 | — |
| A3 | 不同 source_ip 不被去重 | IP A 触发后立即 IP B 触发 | 两次通知（dedupe key 含 source_ip）| — |
| A4 | `ssh_new_ip_login` 仅 first_seen 触发 | S2 触发；S3（first_seen=false）不触发 | 仅 S2 发通知 | — |
| A5 | `exclude_users` 过滤 | 规则 exclude_users=["nagios"] → nagios 触发 first_seen 登录 | 不通知 | — |
| A6 | `exclude_cidrs` 过滤 | 规则 exclude_cidrs=["10.0.0.0/8"] → 10.x 触发 | 不通知 | — |
| A7 | `port_scan_detected` 触发 | preset 卡创建 → 触发 P2 | 通知组收到 | — |
| A8 | validator 拒绝混合 | UI 尝试在同一 alert_rule 里同时配 cpu + ssh_brute_force_detected | 报错 "cannot mix security rule types with other items" | — |
| A9 | validator 拒绝多 security item | API 直接 POST 含 2 个 security item 的 rule | 报错 "only one security item per alert_rule is supported" | — |

## 四、Capability 与权限

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| C1 | 关闭 CAP_SECURITY_EVENTS | settings/capabilities → 关闭 → 等待重连 | agent 收到 `CapabilitiesSync`，停止 watcher；UI 不再收 security_event | — |
| C2 | server 端拒绝事件 | 关 cap 后 agent 还在跑（缓冲场景）| server 收到时静默丢弃 + `audit_log` 出现 `security_event_denied` | — |
| C3 | 重新开启恢复 | 重新打开 cap | watcher 重新启动，事件流恢复 | — |
| C4 | 迁移 backfill 验证 | 升级前 server `capabilities=60` → 升级后 | 自动变 316；非 60 的旧 server 不变 | — |

## 五、其他

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| O1 | 实时 WS 推送 | 开 Security 页 → 触发 S4 | 表格立即出现新行，无需刷新 | — |
| O2 | 高严重度 toast | severity=critical 事件到达 | 浏览器出现 toast 警告 | — |
| O3 | Drawer 查看 evidence | 点击事件行 | Drawer 弹出，evidence JSON 完整 | — |
| O4 | source_ip 一键过滤 | 点击表格里的 IP | 自动填入过滤条 | — |
| O5 | 服务器详情 Security Tab | 进入 `/servers/$id` → Security tab | 显示该 server 最近 50 条 + "View all" 链接 | — |
| O6 | recovery_merge 携带历史 | server 重新绑定后 | 旧 source_id 的 security_event 行 server_id 被更新为 target | — |
| O7 | retention cleanup | 设置 `retention.security_event_days=1`，跑 cleanup task | 1 天前的事件被删 | — |
| O8 | i18n 中英文 | 切换语言 | "Security Events" / "安全事件" 各处标签正确 | — |

## 六、性能与边界

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| B1 | 高频失败抗压 | 在 60s 内触发 1000+ 失败登录 | agent 内存稳定（IP map cap 10000）；事件正常聚合 | — |
| B2 | WS 断连缓冲 | 触发事件期间停 server → 启回 | agent 缓冲 1000 条；恢复后批量重发；超 1000 老的丢弃，warn 日志 | — |
| B3 | RecoveryLock 期间写入 | 触发 recovery → 期间触发 S4 | 事件正常落库（append-only 不受冻结）；recovery_merge 完成后 server_id 跟随 | — |

---

## VPS 自动化冒烟（实测记录）

**环境**：lab VPS `<vps-host>` (Ubuntu 24.04.4 LTS, x86_64, kernel 6.8)
**测试日期**：2026-05-21
**版本**：abuja 分支 commit `8aa03d93`（功能完成时的 HEAD）
**构建**：`cargo build --release -p serverbee-server -p serverbee-agent` → 7m55s；server 60 MB / agent 17 MB

### 执行流程

| # | 步骤 | 关键命令 | 结果 |
|---|---|---|---|
| V1 | 传输源码 | `scp serverbee-abuja-full.tar.gz → /root/`，解压到 `/opt/serverbee-src` | ✅ |
| V2 | 安装依赖 | `apt install build-essential pkg-config libssl-dev sqlite3 conntrack sshpass`；rustup stable | ✅ |
| V3 | 构建 release | `cargo build --release` | ✅ 7m55s |
| V4 | 启动 server | `nohup target/release/serverbee-server > /tmp/server.log` | ✅ 日志见证 migration `m20260521_000024_create_security_event` 执行 |
| V5 | 首次启动密码 | server 控制台打印一次性 admin 密码 | ✅ `cnQJvJUu-...` 由代码生成 |
| V6 | 强制改密 | `POST /api/auth/onboarding`（`must_change_password=true` 状态下只放行此路径）| ✅ |
| V7 | 创建 enrollment | `POST /api/agent/enrollments` → `{ code, expires_at }` | ✅ |
| V8 | 启动 agent | env `ENROLLMENT_CODE` 启动 → 自注册，token 写本地 state | ✅ agent log 见 `WebSocket connected` + `Welcome` |
| V9 | 触发爆破 | `for i in {1..15}; do sshpass -p wrong ssh testuser_brute@127.0.0.1 true; done` | ✅ journal 见 `Failed password for invalid user testuser_brute` |
| V10 | 等待聚合 + 上报 | 等 90s（滑动窗 60s + WS 上报）| ✅ |
| V11 | API 查询 | `GET /api/security/events?event_type=ssh_brute_force&source_ip=127.0.0.1` | ✅ 返回 3 条 `failed_count=10, threshold=10, severity=medium, invalid_user_count=10, sample_users=["testuser_brute"], detector_source="journal"` |
| V12 | 真实攻击侧证 | DB 直查发现 IP `87.251.64.145` 在测试期间被自动捕获 **15 条** ssh_brute_force 事件 | ✅ 端到端管线对真实流量同样有效 |
| V13 | 清理 | 杀进程、删 `/opt/serverbee-src`、删 test DB、恢复原 systemd 服务 | ✅ |

### 验证证据摘录

```
# server log
Applying migration 'm20260521_000024_create_security_event'
Agent ca2c7e6b-... connected from 127.0.0.1:33974

# DB 直查
SELECT event_type, source_ip, COUNT(*) FROM security_event GROUP BY 1,2;
ssh_brute_force | 127.0.0.1       | 3
ssh_brute_force | 87.251.64.145   | 15
```

### 管线全链路确认

`sshd → journalctl → agent journal_watcher → ssh_parser → SshDetector(window=60s, threshold=10) → AgentMessage::SecurityEvent → WS → router/ws/agent.rs → service::security::record_event → security_event 表 + broadcast → REST /api/security/events`

每一跳都有日志或数据证据。

### 已知偏离

- VPS 上已部署的 systemd 服务是 `v0.9.4` 旧版（不含 security 功能），临时停掉跑新构建，测试完恢复
- agent 在 nohup 模式下 stdout 会被 disown race 杀掉，改用 `setsid sh -c "exec ..."`
- SecurityManager 启动后没有 info-level 日志（这是设计如此 —— 一切正常时静默）

### 未在 VPS 跑的子集

- 端口扫描检测（**P2-P6**）：opt-in，需要额外 `CAP_NET_ADMIN` + `conntrack-tools`，本次未启用；机制单测已覆盖
- 告警通知接收链（**A1-A7**）：需要外部 webhook/email；本次仅验证规则匹配 + 落库 + 广播
- 跨 server recovery_merge 实际场景（**B3**）：需要双 agent 模拟身份变更
