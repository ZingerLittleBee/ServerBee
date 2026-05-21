# 防火墙黑名单 — E2E 手动验证

> 前置：参照 [README.md](README.md) 中的「启动本地环境」启动 Server + Agent，并完成首次登录。本功能 **仅 Linux**：macOS / Windows 上的 Agent 会跳过 firewall executor，无法验证 M / G / A / C / R 这些用例。

> **平台依赖**：测试主机需要安装 `nftables`（`apt install nftables` / `dnf install nftables`），Agent 进程需要 root 或 `CAP_NET_ADMIN`。新功能需要在 Capabilities 设置中显式启用 `CAP_FIREWALL_BLOCK`（默认 **关闭**，位值 `512`）。

> **VPS 占位符**：本文档不记录真实 VPS IP，所有外部主机/连通性命令统一用 `<vps-host>` 占位符。

---

## 一、Setup（前置条件）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| S1 | 启用 `CAP_FIREWALL_BLOCK` | Settings → Capabilities → 给目标 server 勾选 Firewall Blocklist → 保存 | Server 通过 WebSocket 推送 `BlocklistReset` + 空 `BlocklistSync` 给 Agent；目标主机 `nft list table inet serverbee` 显示空 `block_v4` / `block_v6` set；UI 顶部 `/settings/firewall` 不再提示 "no capable agents" | — |

---

## 二、Manual CRUD（手动黑名单增删）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| M1 | 添加单 IP | Settings → Firewall → Add block，目标 `198.51.100.5`，`cover_type=all`，备注随意 → Save | UI 列表新增一行；toast 成功；目标主机 `nft list set inet serverbee block_v4` 包含 `198.51.100.5`；Activity 列表出现 `firewall_block_created` + `firewall_block_applied_agent` | — |
| M2 | 验证连通性被切断 | 从 `<vps-host>` 之外的网络（即 `198.51.100.5` 同段的主机）尝试 `curl --connect-timeout 3 http://<vps-host>:22`（或任意已开放端口） | 连接超时 / RST | — |
| M3 | 删除单条记录 | UI 列表选中 M1 的行 → Delete | `nft list set inet serverbee block_v4` 不再包含 `198.51.100.5`；M2 的 `curl` 重新成功；Activity 列表出现 `firewall_block_deleted` + `firewall_block_removed_agent` | — |
| M4 | 添加 CIDR | Add block `203.0.113.0/24` → Save | `nft list set inet serverbee block_v4` 显示 `203.0.113.0/24` 区间（`flags interval`） | — |
| M5 | 重复添加去重 | 立即再次 POST 同一目标 `203.0.113.0/24` | 返回 `409 target ... already blocked`，列表无新行 | — |

---

## 三、Guardrails（Tier-1 / Tier-2 护栏）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| G1 | Tier-1 拒绝 loopback | Add block `127.0.0.1` | `409`，body 包含 `loopback`；Audit 写入 `firewall_block_rejected_server` 含原因 | — |
| G2 | Tier-2 拒绝 Agent 自身外网 IP | 在已识别 Agent 外网 IP 的前提下，尝试 Add block 该 IP | `409`，body 包含 `allow_list` 或 `agent external IP`；列表无新行 | — |
| G3 | Tier-2 拒绝 `firewall.allow_list` 命中 | 启动 Server 时设置 `SERVERBEE_FIREWALL__ALLOW_LIST="203.0.113.5"`，再 Add block `203.0.113.5` | `409`，body 包含 `allow_list` | — |

---

## 四、Auto-block（告警自动封禁）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| A1 | 创建带 `block_source_ip` 的爆破规则 | Settings → Alerts → 创建 `ssh_brute_force_detected` 规则，动作勾选 `Block source IP`，`cover_type=all` → Save | 规则保存成功；列表显示 `action: block_source_ip` 标记 | — |
| A2 | 触发爆破并自动封禁 | 从测试机对 `<vps-host>` 发起 15+ 次 SSH 失败：`for i in {1..15}; do sshpass -p wrong ssh root@<vps-host> true; done` | 90 秒内：1）Security Events 出现 `ssh_brute_force` 事件；2）Firewall 列表出现 `origin=auto`、`origin_event_id` 非空的新行；3）`nft list set inet serverbee block_v4` 包含攻击者 IP；4）Activity 含 `firewall_block_created` (origin=auto) | — |
| A3 | 同源 IP 重复触发去重 | 等告警 dedupe 窗口外（默认 60s 后）再触发一次 A2 | Firewall 列表 **不** 新增行（规范化目标去重）；Audit 不出现新的 `firewall_block_created` | — |
| A4 | 已有记录但不覆盖触发 server 时跳过 | 先手动加一条只覆盖另一台 server 的 block（`cover_type=selected`，选 server B）；从攻击者 IP 对当前 server A 触发爆破 | Firewall 列表 **不** 新增行（不会扩大已有记录覆盖）；Audit 写入 `firewall_auto_block_skipped_conflict`，含已有 block id 与触发 server id | — |

---

## 五、Capability transitions（能力位切换）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| C1 | 关闭 `CAP_FIREWALL_BLOCK` | 维持 3 条有效 block，Settings → Capabilities → 取消勾选 → 保存 | Server 推送 `BlocklistReset` → Agent ack → `nft list ruleset` **不再** 包含 `inet serverbee` 表；UI Firewall 行保留（Server 端数据未删），但 `/settings/firewall` 顶部提示 capability 关闭 | — |
| C2 | 重新启用 | 再次勾选 `CAP_FIREWALL_BLOCK` → 保存 | Server 推送 `BlocklistReset` 后立刻发完整 `BlocklistSync`；3 条记录全部重新出现在 `nft list set inet serverbee block_v4`；Activity 含 3 条 `firewall_block_applied_agent` | — |

---

## 六、Resilience（断连 / 重启恢复）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| R1 | Agent 重启 | 保留 3 条 block，重启 `serverbee-agent` 进程 | Agent 启动后 bootstrap 重新拉表；`nft list table inet serverbee` 含相同 3 条；Activity 出现新一轮 `firewall_block_applied_agent` | — |
| R2 | Server 重启 | 保留 3 条 block，停止再启动 Server | Agent 重连后 Server 触发全量 sync；in-memory apply state 重建；Activity 显示 fresh `firewall_block_applied_agent` 条目；`nft` 内容保持一致 | — |
| R3 | nftables 不可用 → 关闭 cap 时 ack 失败 | 在主机 `systemctl stop nftables.service` 并 `rmmod nf_tables nf_tables_ipv4` 模拟内核不可用 → UI 关闭 `CAP_FIREWALL_BLOCK` | Agent ack `BlocklistResetAck { ok: false, reason: "nft kernel module unavailable" }`（或类似）；Audit 写入 `firewall_reset_failed_agent` 含原因；UI Activity 显示失败原因 | — |

---

## 七、Permission checks（成员权限）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| P1 | Member POST 被拒 | 以 member 角色登录 → `POST /api/firewall/blocks` 任意 payload | `403`，body 含 admin 要求 | — |
| P2 | Member DELETE 被拒 | 以 member 角色 → `DELETE /api/firewall/blocks/{id}` | `403` | — |
| P3 | Member GET 允许 | 以 member 角色 → `GET /api/firewall/blocks` | `200`，返回列表（只读） | — |

---

## VPS 自动化冒烟（实测记录）

> 本节由 Phase 5 Task 5.4 在真机上填写。VPS 占位符 `<vps-host>`，**不要写真实 IP**。
