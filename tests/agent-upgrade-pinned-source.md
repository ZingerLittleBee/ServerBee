# Pinned-Source 自升级测试用例

Agent 自升级现在从本地配置的固定来源下载，Server 发来的 `download_url`/`sha256` 已被废弃忽略。本清单覆盖新来源决策逻辑、防降级、TLS 加固、SPKI pin、配置优先级和向后兼容场景。

## 前置条件

参照 [README.md](README.md) 中的「启动本地环境」部分完成 Server + Agent 启动和管理员登录。

Agent 必须已注册并持有 `CAP_UPGRADE` 能力（新注册默认包含）。

```bash
# 查看当前 agent 版本（用于后续防降级测试）
curl -s -b /tmp/sb-cookies.txt http://localhost:9527/api/servers \
  | python3 -m json.tool | grep -A2 '"version"'

# 快速确认 CAP_UPGRADE 已启用（capabilities & 4 != 0）
curl -s -b /tmp/sb-cookies.txt http://localhost:9527/api/servers \
  | python3 -m json.tool | grep '"capabilities"'
```

> **配置优先级（高 → 低）**：CLI `--release-repo` > 环境变量 `SERVERBEE_UPGRADE__RELEASE_REPO_URL` > `agent.toml` `[upgrade] release_repo_url` > 编译时 `SERVERBEE_RELEASE_REPO` > 默认官方仓库 `https://github.com/ZingerLittleBee/ServerBee/releases`

---

## 一、默认来源升级（官方 GitHub Release）

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| PS-1 | UI 触发升级下载来自官方仓库 | 1. 确认 `agent.toml` 无 `[upgrade]` 节（或 `release_repo_url` 未设置）<br>2. 在 `/servers/:id` 点击"Upgrade Agent"<br>3. 选择比当前版本更高的版本号并确认 | 升级进度面板出现；Agent 日志显示从 `https://github.com/ZingerLittleBee/ServerBee/releases/download/v{version}/{asset}` 下载 binary，从 `.../checksums.txt` 下载校验文件；哈希比对通过；binary 替换；Agent 以新版本重启 | ⬜ |
| PS-2 | 升级阶段依次出现 | 观察进度面板 | 按序出现 `downloading → verifying → installing → restarting`；每阶段状态图标和文案正确 | ⬜ |
| PS-3 | 升级完成后版本号更新 | 等待 Agent 重连（通常 10–30 秒） | 服务器详情页 header 版本号更新为目标版本；无需手动刷新（WebSocket 实时推送） | ⬜ |

---

## 二、自定义来源（镜像 / 私有 release 仓库）

**前置**：准备一个镜像地址，目录结构与 GitHub Releases 相同（`download/v{version}/{asset}` 和 `download/v{version}/checksums.txt`），通过 HTTPS 可访问。

**`agent.toml` 配置**：

```toml
[upgrade]
release_repo_url = "https://mirror.example.com/serverbee/releases"
```

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| PS-4 | `agent.toml` 自定义来源生效 | 1. 写入上述配置并重启 Agent<br>2. 触发 UI 升级 | Agent 日志显示 `release_repo_url` 为镜像地址；binary 从镜像下载；其余流程同 PS-1 | ⬜ |
| PS-5 | Server 发来的 `download_url`/`sha256` 被忽略 | 抓包确认 Server WebSocket 发出含 `download_url` 字段的 `Upgrade` 消息（旧协议） | Agent 日志无"使用 download_url"字样；下载仍指向本地配置来源 | ⬜ |

---

## 三、CLI `--release-repo` 覆盖（最高优先级）

**前置**：`agent.toml` 已设置某 `release_repo_url`；同时设置环境变量 `SERVERBEE_UPGRADE__RELEASE_REPO_URL`。

```bash
# 以 CLI 覆盖启动
cargo run -p serverbee-agent -- --release-repo https://cli-mirror.example.com/releases
```

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| PS-6 | CLI 覆盖优先于配置文件和环境变量 | 1. `agent.toml` 设置一个来源 A<br>2. 环境变量设置来源 B<br>3. CLI `--release-repo` 传入来源 C<br>4. 触发升级 | Agent 启动日志输出 `release_repo_url overridden by --release-repo CLI flag`；下载来自来源 C | ⬜ |
| PS-7 | 环境变量覆盖配置文件，但被 CLI 覆盖 | 1. `agent.toml` 设置来源 A；不传 CLI 参数<br>2. `SERVERBEE_UPGRADE__RELEASE_REPO_URL=https://env-mirror.example.com/releases` 启动<br>3. 触发升级 | 下载来自环境变量 URL（来源 B），而不是 `agent.toml`（来源 A） | ⬜ |
| PS-8 | `--release-repo=<url>`（等号形式）同样生效 | `cargo run -p serverbee-agent -- --release-repo=https://eq-mirror.example.com/releases` | 同 PS-6，覆盖生效 | ⬜ |

---

## 四、防降级：Server 触发版本 ≤ 当前版本时被拒绝

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| PS-9 | 相同版本被拒绝 | 通过 API 直接发送与当前 Agent 版本相同的升级请求：<br>`curl -s -b /tmp/sb-cookies.txt -X POST http://localhost:9527/api/servers/:id/upgrade -H 'Content-Type: application/json' -d '{"version":"<current_version>"}'` | Agent 日志出现 `refusing non-upgrade: target X.Y.Z is not greater than current X.Y.Z`；进度面板显示升级失败（`UpgradeStatus::Failed`，stage `Downloading`）；binary 未被替换 | ⬜ |
| PS-10 | 降级版本被拒绝 | 同上，发送低于当前版本的版本号 | 同 PS-9 拒绝逻辑；binary 未被替换 | ⬜ |
| PS-11 | 高版本正常通过 | 发送严格大于当前版本的版本号 | 防降级检查通过，继续后续下载流程 | ⬜ |

---

## 五、SPKI Pin

**获取目标服务器 SPKI SHA-256 的参考命令（用于生成正确 pin）**：

```bash
# 取 github.com 的 leaf cert SPKI SHA-256（示例；实际 pin 以自建工具为准）
openssl s_client -connect github.com:443 </dev/null 2>/dev/null \
  | openssl x509 -noout -pubkey \
  | openssl pkey -pubin -outform DER \
  | openssl dgst -sha256 -hex | awk '{print $2}'
```

**`agent.toml` SPKI 配置**：

```toml
[upgrade]
release_repo_url    = "https://github.com/ZingerLittleBee/ServerBee/releases"
release_cert_spki_sha256 = "<64-hex-chars>"
```

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| PS-12 | 正确 SPKI pin → 升级成功 | 配置与 release host 实际 leaf SPKI 匹配的 64-hex `release_cert_spki_sha256`，触发升级 | TLS 握手通过 SPKI 比对；升级正常完成 | ⬜ |
| PS-13 | 错误 SPKI pin → TLS 失败，升级失败 | 配置一个全 `a` 的伪 SPKI pin（`aaaa...aa` 64 位），触发升级 | Agent 日志出现 `SPKI pin mismatch for release host`（rustls error）；升级报错 `UpgradeStatus::Failed`；binary 未被替换 | ⬜ |
| PS-14 | 格式非法的 SPKI pin → 启动即 fail-fast | `release_cert_spki_sha256 = "badpin"`（非 64 位 hex），启动 Agent 并触发升级 | Agent 在 `perform_upgrade` 内 `normalize_spki_pin` 失败；日志出现 `invalid SPKI pin: release_cert_spki_sha256 must be 64 lowercase hex chars`；升级报错 `UpgradeStatus::Failed`（stage `Downloading`）；Agent 进程本身仍运行（非致命） | ⬜ |
| PS-15 | 空 SPKI pin（未设置）→ 不启用 pin | `release_cert_spki_sha256` 未设置或为空字符串，触发升级 | 无 SPKI 比对，使用标准 webpki 根证书验证；升级正常进行 | ⬜ |

---

## 六、恶意/被攻陷 Server 模拟

**场景说明**：攻陷的 Server 可能伪造 `Upgrade` 消息，植入恶意 `download_url` 和 `sha256` 以诱导 Agent 下载恶意 binary。本特性的防护点是：Agent **忽略** Server 提供的这两个字段，始终从本地 pinned 来源推导 URL，并对比 pinned 来源的 `checksums.txt`。

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| PS-16 | 伪造 `download_url` 被忽略 | 修改 Server 代码（测试用），让 `trigger_upgrade` 发送含 `download_url: "https://evil.example.com/malware"` 的 Upgrade 消息；重启 Server 并触发升级 | Agent 日志无访问 `evil.example.com` 记录；下载仍来自本地配置的 pinned 来源；正常流程继续 | ⬜ |
| PS-17 | 伪造 `sha256` 被忽略 | Server 发送 `sha256: "deadbeef..."`（任意值）；checksums.txt 来自 pinned 来源 | Agent 使用 pinned 来源的 `checksums.txt` 做哈希验证，不使用 Server 提供的 `sha256`；验证逻辑正确，升级按预期成功或失败（取决于实际哈希） | ⬜ |

---

## 七、配置文件权限告警

**仅 Unix**（Linux/macOS）。

```bash
# 造出 group-writable 的 agent.toml
chmod 664 agent.toml
```

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| PS-18 | group-writable `agent.toml` → 启动时 WARN 但继续运行 | `chmod 664 agent.toml`，重启 Agent | 启动日志出现 `SECURITY: agent.toml is group/world-writable (mode 664); another local user could tamper release_repo_url. Run: chmod 600 agent.toml`；Agent 正常启动，不退出 | ⬜ |
| PS-19 | world-writable `agent.toml` → 同样 WARN 不退出 | `chmod 666 agent.toml`，重启 Agent | 同 PS-18，mode 显示 `666` | ⬜ |
| PS-20 | `chmod 600` 后无告警 | `chmod 600 agent.toml`，重启 Agent | 启动日志无 `SECURITY:` 字样 | ⬜ |
| PS-21 | `/etc/serverbee/agent.toml` 权限同样被检查 | `chmod 664 /etc/serverbee/agent.toml`，重启 Agent | 路径 `/etc/serverbee/agent.toml` 出现在 WARN 日志中 | ⬜ |

---

## 八、向后兼容：旧版 Agent 对接新版 Server

**场景说明**：旧版 Agent（pinned-source 特性上线前，协议版本 < sofia-v1）期望 Server 在 `Upgrade` 消息中提供 `download_url`，但新 Server 只发送 `version`。此 breaking change 导致旧 Agent 无法自动升级。

| # | 测试场景 | 操作步骤 | 预期结果 | 状态 |
|---|---------|---------|---------|------|
| PS-22 | 旧版 Agent + 新版 Server → 自动升级失败 | 1. 将旧版（v0.9.1 或更早）Agent 二进制连接到新版 Server<br>2. 在 UI 触发升级 | 旧 Agent 收到不含 `download_url` 的 `Upgrade` 消息；无法推导下载地址；升级失败或报错；旧 Agent 继续运行当前版本 | ⬜ |
| PS-23 | 需手动一次性重装 | 依照文档（`ENV.md` / Configuration 文档的 breaking change 说明）手动在目标服务器重新安装最新 Agent | 新 Agent 成功连接 Server，支持 pinned-source 升级；此后 UI 触发升级正常工作 | ⬜ |

> **注意**：此 breaking change 仅影响升级路径，Agent 的监控上报功能不受影响。旧 Agent 仍可正常上报指标，只是无法通过 UI 升级到新版，必须手动重装一次。

---

## 相关文件

- `crates/agent/src/upgrade.rs` — URL 推导、防降级、TLS client、SPKI pin、重定向决策
- `crates/agent/src/config.rs` — `UpgradeConfig`（`release_repo_url`、`release_cert_spki_sha256`）
- `crates/agent/src/main.rs` — `--release-repo` CLI 解析、文件权限告警
- `crates/agent/src/reporter.rs` — `perform_upgrade` 实现
- `crates/common/src/protocol.rs` — `ServerMessage::Upgrade`（`download_url`/`sha256` 标注废弃）、`UpgradeStage`、`UpgradeStatus`
- `apps/web/src/components/server/upgrade-panel.tsx` — 升级进度 UI
