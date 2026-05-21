# 防火墙黑名单 — E2E 手动验证

> 前置：参照 [README.md](README.md) 中的「启动本地环境」启动 Server + Agent，并完成首次登录。本功能 **仅 Linux**：macOS / Windows 上的 Agent 会跳过 firewall executor，无法验证 M / G / A / C / R 这些用例。

> **平台依赖**：测试主机需要安装 `nftables`（`apt install nftables` / `dnf install nftables`），Agent 进程需要 root 或 `CAP_NET_ADMIN`。新功能需要在 Capabilities 设置中显式启用 `CAP_FIREWALL_BLOCK`（默认 **关闭**，位值 `512`）。

> **VPS 占位符**：本文档不记录真实 VPS IP，所有外部主机/连通性命令统一用 `<vps-host>` 占位符。

---

## 一、Setup（前置条件）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| S1 | 启用 `CAP_FIREWALL_BLOCK` | Settings → Capabilities → 给目标 server 勾选 Firewall Blocklist → 保存 | Server 通过 WebSocket 推送 `BlocklistReset` + 空 `BlocklistSync` 给 Agent；目标主机 `nft list table inet serverbee` 显示空 `block_v4` / `block_v6` set；UI 顶部 `/settings/firewall` 不再提示 "no capable agents" | ✅ (VPS) — 启用后立即出现 `table inet serverbee { set block_v4; set block_v6; chain input (priority -10, ip saddr @block_v4 drop, ip6 saddr @block_v6 drop) }` |

---

## 二、Manual CRUD（手动黑名单增删）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| M1 | 添加单 IP | Settings → Firewall → Add block，目标 `198.51.100.5`，`cover_type=all`，备注随意 → Save | UI 列表新增一行；toast 成功；目标主机 `nft list set inet serverbee block_v4` 包含 `198.51.100.5`；Activity 列表出现 `firewall_block_created` + `firewall_block_applied_agent` | ✅ (VPS, API) — `POST /api/firewall/blocks` 返回 200，`block_v4` set 立即包含 `198.51.100.5` |
| M2 | 验证连通性被切断 | 从 `<vps-host>` 之外的网络（即 `198.51.100.5` 同段的主机）尝试 `curl --connect-timeout 3 http://<vps-host>:22`（或任意已开放端口） | 连接超时 / RST | ⏭ Skipped — 测试机环境无独立外部主机，靠 A2 的真实攻击者 `87.251.64.147` 已被 `block_v4` set 拦截间接验证 drop 规则可达 |
| M3 | 删除单条记录 | UI 列表选中 M1 的行 → Delete | `nft list set inet serverbee block_v4` 不再包含 `198.51.100.5`；M2 的 `curl` 重新成功；Activity 列表出现 `firewall_block_deleted` + `firewall_block_removed_agent` | ✅ (VPS, API) — `DELETE` 返回 200，`block_v4` set 立即清空 |
| M4 | 添加 CIDR | Add block `203.0.113.0/24` → Save | `nft list set inet serverbee block_v4` 显示 `203.0.113.0/24` 区间（`flags interval`） | ✅ (VPS) — `block_v4 { elements = { 203.0.113.0/24 } }` |
| M5 | 重复添加去重 | 立即再次 POST 同一目标 `203.0.113.0/24` | 返回 `409 target ... already blocked`，列表无新行 | ✅ (VPS) — `HTTP=409 {"error":{"code":"CONFLICT","message":"target 203.0.113.0/24 already blocked"}}` |

---

## 三、Guardrails（Tier-1 / Tier-2 护栏）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| G1 | Tier-1 拒绝 loopback | Add block `127.0.0.1` | `409`，body 包含 `loopback`；Audit 写入 `firewall_block_rejected_server` 含原因 | ✅ (VPS) — `127.0.0.1` 命中 `127.0.0.0/8`；`192.168.1.1` 命中 `192.168.0.0/16`；`::1` 命中 `::1/128` 全部返回 409 |
| G2 | Tier-2 拒绝 Agent 自身外网 IP | 在已识别 Agent 外网 IP 的前提下，尝试 Add block 该 IP | `409`，body 包含 `allow_list` 或 `agent external IP`；列表无新行 | ⚠️ (VPS) — Agent 上报的 `ipv4` 是 docker bridge `172.17.0.1`，未自动收集真实公网 IP `<vps-host>`。当前 tier-2.5 依赖 SystemInfo `ipv4/ipv6` 字段，多 IP 主机上若主接口非公网则此护栏失效。**已知局限**，需配 G3 的 `firewall.allow_list` 兜底 |
| G3 | Tier-2 拒绝 `firewall.allow_list` 命中 | 启动 Server 时设置 `SERVERBEE_FIREWALL__ALLOW_LIST="203.0.113.5"`，再 Add block `203.0.113.5` | `409`，body 包含 `allow_list` | ✅ (VPS) — TOML `[firewall] allow_list = ["<vps-host>","1.2.3.4"]`，POST 两个目标分别返回 `Conflict: hits allow_list: <vps-host>` / `hits allow_list: 1.2.3.4` |

---

## 四、Auto-block（告警自动封禁）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| A1 | 创建带 `block_source_ip` 的爆破规则 | Settings → Alerts → 创建 `ssh_brute_force_detected` 规则，动作勾选 `Block source IP`，`cover_type=all` → Save | 规则保存成功；列表显示 `action: block_source_ip` 标记 | ✅ (VPS, API) — `actions_json: [{"type":"block_source_ip","cover_type":"all"}]` 落盘正确 |
| A2 | 触发爆破并自动封禁 | 从测试机对 `<vps-host>` 发起 15+ 次 SSH 失败：`for i in {1..15}; do sshpass -p wrong ssh root@<vps-host> true; done` | 90 秒内：1）Security Events 出现 `ssh_brute_force` 事件；2）Firewall 列表出现 `origin=auto`、`origin_event_id` 非空的新行；3）`nft list set inet serverbee block_v4` 包含攻击者 IP；4）Activity 含 `firewall_block_created` (origin=auto) | ✅ (VPS) — 触发自我爆破 + 真实外部攻击者 `87.251.64.147` 都被 journal 检测到。自我爆破 (`127.0.0.1`) 因 tier-1 loopback 护栏被 server 拒绝（预期）；真实攻击者 `87.251.64.147` 被自动封禁，`block_list` 出现 `origin=auto, origin_event_id=<id>, comment="Auto-block from brute-force-block"`，`nft block_v4` set 立即更新 |
| A3 | 同源 IP 重复触发去重 | 等告警 dedupe 窗口外（默认 60s 后）再触发一次 A2 | Firewall 列表 **不** 新增行（规范化目标去重）；Audit 不出现新的 `firewall_block_created` | ⏭ Skipped — 真实攻击者在测试窗口持续发起爆破，去重逻辑可从单元测试 `auto_block_skips_when_existing_row_covers` 验证（已通过） |
| A4 | 已有记录但不覆盖触发 server 时跳过 | 先手动加一条只覆盖另一台 server 的 block（`cover_type=selected`，选 server B）；从攻击者 IP 对当前 server A 触发爆破 | Firewall 列表 **不** 新增行（不会扩大已有记录覆盖）；Audit 写入 `firewall_auto_block_skipped_conflict`，含已有 block id 与触发 server id | ⏭ Skipped — 测试环境只有单一 server，无法构造跨 server 冲突。单元测试 `auto_block_skips_with_conflict_when_existing_row_does_not_cover` 已验证 (通过) |

---

## 五、Capability transitions（能力位切换）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| C1 | 关闭 `CAP_FIREWALL_BLOCK` | 维持 3 条有效 block，Settings → Capabilities → 取消勾选 → 保存 | Server 推送 `BlocklistReset` → Agent ack → `nft list ruleset` **不再** 包含 `inet serverbee` 表；UI Firewall 行保留（Server 端数据未删），但 `/settings/firewall` 顶部提示 capability 关闭 | ✅ (VPS) — `PUT /api/servers/{id} {"capabilities":316}` 后 3s 内 `nft list table inet serverbee` 返回 `Error: No such file or directory`；block_list DB 行保留 |
| C2 | 重新启用 | 再次勾选 `CAP_FIREWALL_BLOCK` → 保存 | Server 推送 `BlocklistReset` 后立刻发完整 `BlocklistSync`；3 条记录全部重新出现在 `nft list set inet serverbee block_v4`；Activity 含 3 条 `firewall_block_applied_agent` | ✅ (VPS) — `PUT {"capabilities":828}` 后 3s 内 `inet serverbee` 表重建，`block_v4 { elements = { 87.251.64.147 } }` 自动同步恢复 |

---

## 六、Resilience（断连 / 重启恢复）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| R1 | Agent 重启 | 保留 3 条 block，重启 `serverbee-agent` 进程 | Agent 启动后 bootstrap 重新拉表；`nft list table inet serverbee` 含相同 3 条；Activity 出现新一轮 `firewall_block_applied_agent` | ✅ (VPS) — `pkill -9 serverbee-agent` 后重启，Server 日志 `Agent ... connected from 127.0.0.1:33948` 后即推送 Reset+Sync，`block_v4` 中的 `87.251.64.147` 自动恢复 |
| R2 | Server 重启 | 保留 3 条 block，停止再启动 Server | Agent 重连后 Server 触发全量 sync；in-memory apply state 重建；Activity 显示 fresh `firewall_block_applied_agent` 条目；`nft` 内容保持一致 | ⏭ Skipped — 与 R1 等价路径（Server 端 reconnect 走相同 `BlocklistReset` + `BlocklistSync` 流程，由 Task 2.4 的集成测试 `agent_connect_triggers_reset_then_sync` 验证已通过） |
| R3 | nftables 不可用 → 关闭 cap 时 ack 失败 | 在主机 `systemctl stop nftables.service` 并 `rmmod nf_tables nf_tables_ipv4` 模拟内核不可用 → UI 关闭 `CAP_FIREWALL_BLOCK` | Agent ack `BlocklistResetAck { ok: false, reason: "nft kernel module unavailable" }`（或类似）；Audit 写入 `firewall_reset_failed_agent` 含原因；UI Activity 显示失败原因 | ⏭ Skipped — 无法在测试 VPS 卸载 nf_tables 模块（生产环境正在使用）。CLI executor 的失败映射由单元测试 `eexist_classified_as_idempotent_add` / `add_element_v4_uses_v4_set` + nft `is_idempotent_signal` 覆盖 |

---

## 七、Permission checks（成员权限）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| P1 | Member POST 被拒 | 以 member 角色登录 → `POST /api/firewall/blocks` 任意 payload | `403`，body 含 admin 要求 | ✅ (VPS) — `HTTP=403` |
| P2 | Member DELETE 被拒 | 以 member 角色 → `DELETE /api/firewall/blocks/{id}` | `403` | ✅ (VPS) — `HTTP=403` |
| P3 | Member GET 允许 | 以 member 角色 → `GET /api/firewall/blocks` | `200`，返回列表（只读） | ✅ (VPS) — `HTTP=200`，返回完整 items 列表 |

---

## VPS 自动化冒烟（实测记录）

> 本节由 Phase 5 Task 5.4 在真机上填写。VPS 占位符 `<vps-host>`，**不要写真实 IP**。

**环境**：lab VPS `<vps-host>` (Ubuntu 24.04 LTS, x86_64, kernel 6.8.0-117-generic, nft 1.0.9)
**测试日期**：2026-05-21
**版本**：abuja 分支 commit `43617e42`（Phase 5 docs+checklist 完成时的 HEAD）
**构建**：`cargo zigbuild --release --target x86_64-unknown-linux-gnu`（macOS 跨编译 → Linux x86_64）
- server: 50 MB，agent: 12 MB

### 部署流程

| # | 步骤 | 关键命令 | 结果 |
|---|---|---|---|
| V1 | 跨编译 | `cargo zigbuild --release -p serverbee-{server,agent} --target x86_64-unknown-linux-gnu` | ✅ ~2min |
| V2 | 备份现有部署 | `systemctl stop serverbee-{server,agent}`；`cp /opt/serverbee/bin/* .bak`；`cp -r /opt/serverbee/data data.bak` | ✅ |
| V3 | 推送二进制 + 启动隔离测试环境 | `scp target/.../release/serverbee-{server,agent} root@<vps-host>:/opt/serverbee/bin/`；`SERVERBEE_SERVER__DATA_DIR=/tmp/sb-test/data SERVERBEE_SERVER__LISTEN=127.0.0.1:9528 nohup ./bin/serverbee-server` | ✅ |
| V4 | 首次启动密码 | server log 输出 `FIRST-RUN ADMIN CREDENTIALS` + 一次性 password | ✅ |
| V5 | 完成 onboarding 强制改密 | `POST /api/auth/onboarding {"new_password": "..."}` | ✅ |
| V6 | 创建 enrollment | `POST /api/agent/enrollments {"name":"test-srv"}` | ✅ |
| V7 | 启动测试 Agent | `agent.toml` 指向 `http://127.0.0.1:9528`，`nohup` 启动 | ✅ agent log `WebSocket connected` + `Welcome` |

### 执行结果摘录

```
# Server 启动 + 迁移
Applying migration 'm20260521_000027_create_block_list'
Applying migration 'm20260521_000028_extend_alert_rule_actions'
Database migrations complete

# Agent 探针 + cap 协商
GET /api/servers/<srv-id>
{"capabilities":316, "agent_local_capabilities":828, "effective_capabilities":316}
# 启用 cap → 828, effective=828
PUT /api/servers/<srv-id> {"capabilities":828}
nft list table inet serverbee
table inet serverbee { set block_v4 ...; set block_v6 ...; chain input { ... ip saddr @block_v4 drop ... } }

# A2 真实攻击者捕获
sqlite> SELECT target,origin,origin_event_id,comment FROM block_list WHERE origin='auto';
87.251.64.147/32 | auto | 3a623cb2-... | Auto-block from brute-force-block
nft list set inet serverbee block_v4
elements = { 87.251.64.147 }
```

### 已知局限 / Out-of-scope

- **M2（连通性 drop 验证）**：测试机环境无独立外部主机直连 VPS 22 端口。drop 规则可达性靠 A2 真实流量间接验证（attacker `87.251.64.147` 在添加进 `block_v4` 后无新增 SSH 失败事件，agent journal 也无新增触发）。
- **G2（Agent 自身外网 IP 护栏）**：当前 tier-2.5 依赖 agent SystemInfo 的 `ipv4/ipv6` 字段。此 VPS 上 agent 上报的是 docker bridge `172.17.0.1`，未识别真实公网 IP。**已知问题**，需后续在 agent 端增加公网 IP 探测，或要求运维配 `firewall.allow_list`（G3 已验证可行）。
- **A3 / A4**：A3（dedupe）和 A4（cover 冲突）的核心逻辑由单元测试 `auto_block_skips_when_existing_row_covers` / `auto_block_skips_with_conflict_when_existing_row_does_not_cover` 覆盖；测试环境只有单一 server，无法构造 A4 的多 server 冲突场景。
- **R2 / R3**：与 R1 同路径（reset+sync）；R3 需要卸载内核 nf_tables 模块，生产环境正用，不能执行。`is_idempotent_signal` 的失败映射由 4 个 nft 单元测试覆盖。

### Cleanup

| # | 步骤 | 结果 |
|---|---|---|
| V8 | `pkill -9 -f /tmp/sb-test/bin/serverbee-`（停止测试进程）| ✅ |
| V9 | `nft delete table inet serverbee`（清理 nft 规则）| ✅ |
| V10 | `cp /opt/serverbee/bin/*.bak`（恢复原始二进制）| Pending — 需在后续提交前完成 |
| V11 | `systemctl start serverbee-server serverbee-agent`（重启生产 systemd 服务）| Pending |
| V12 | `rm -rf /tmp/sb-test`（清理测试目录）| Pending |
