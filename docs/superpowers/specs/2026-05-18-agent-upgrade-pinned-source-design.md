# Agent 自升级 Pinned-Source 安全加固设计

- 日期: 2026-05-18
- 状态: 已批准设计,待写实现计划
- 关联安全发现: 自升级无来源信任锚 → 全 fleet RCE(High)

## 1. 背景与问题

当前 Agent 自升级流程(`crates/agent/src/reporter.rs::perform_upgrade` +
`crates/server/src/service/upgrade_release.rs` + `crates/server/src/router/api/server.rs:591`):

```
Server 下发 ServerMessage::Upgrade { version, download_url, sha256 }
  → Agent 下载 download_url
  → 校验下载内容 SHA-256 == Server 给的 sha256
  → 替换二进制并重启
```

`download_url` 与 `sha256` **都来自 Server**。SHA-256 在此只证明传输完整性,不证明来源真实性——
没有任何 Server 不掌握的密钥参与。一个被攻陷或可中间人(MITM)的 Server 可以下发
`download_url = 攻击者控制的恶意二进制` + `sha256 = 该恶意二进制的哈希`,Agent 校验 100% 通过。

CAP_UPGRADE 默认开启,因此这是**无需预先开启 exec 即可实现全 fleet 远程接管**的链路。
本设计在保留 CAP_UPGRADE 默认开启的前提下根治该问题。

### 1.1 方案选择(已决策)

代码签名(离线私钥 / Sigstore)是理论上最完整的方案,但引入私钥治理、CI 签名、密钥轮换等
长期运维负担。经权衡,本项目作为轻量自托管监控系统,采用 **pinned-source(来源锁定)**
方案:把"决定从哪里下载"的权力从不可信的 Server 收回到宿主机所有者控制的可信配置链。
代码签名作为未来可选增强,**本次不做**。

## 2. 信任模型与威胁边界

### 2.1 可信输入(决定下载来源)

下列输入由宿主机 root / Agent 进程所有者控制,视为可信。优先级从低到高:

1. 编译期默认常量(可由 fork 自托管者在构建时用 `SERVERBEE_RELEASE_REPO` 环境变量烤入自己的默认值)
2. `/etc/serverbee/agent.toml` 的 `[upgrade]` 段
3. `agent.toml` 的 `[upgrade]` 段
4. 环境变量 `SERVERBEE_UPGRADE__RELEASE_REPO_URL`
5. CLI 参数 `--release-repo <url>`(最高优先级)

### 2.2 不可信输入

WebSocket `ServerMessage::Upgrade`。Server **只能提供 `version` 字符串**。
`download_url` / `sha256` 字段 Agent 一律忽略。Server 可能被攻陷或被中间人。

### 2.3 本方案根治的攻击

- 被攻陷 / MITM 的 Server 把 `download_url` 指向攻击者控制的恶意二进制
- 被攻陷 / MITM 的 Server 伪造 `sha256`
- 被攻陷 Server 把 `version` 设为某个真实存在但有已知漏洞的旧版本(降级攻击)
- 企业代理 / 杀毒软件 / 本地注入根 CA 对 Agent → pinned 主机的 TLS 中间人(由 §2.4 加固覆盖)

### 2.4 TLS 中间人加固

升级下载使用专用 HTTP client,具备两层防护:

- **捆绑根证书库**:仅信任 Mozilla 内置根证书库(webpki-roots),忽略操作系统信任库。
  干掉现实中最常见的一类 MITM(企业代理、杀软 TLS 拦截、本地注入根 CA),对 github.com
  默认源同样有效。
- **可选 SPKI pinning**:配置项 `release_cert_spki_sha256`(默认空=不启用)。设置后,
  自定义 `ServerCertVerifier` 在标准证书链校验**之后**追加比对 leaf 证书 SPKI 的
  SHA-256。主要服务于自托管者锁定自己镜像的证书。**不对 github.com 默认源使用**
  (GitHub leaf + Fastly CDN 证书频繁轮换,pin 会随轮换失效)。

### 2.5 已知残留风险(spec 明确接受)

- 合法公共 CA 被胁迫 / 被攻陷,专门为目标主机错签证书(国家级 / CA 级攻击)
- pinned 仓库 / GitHub 账号本身被攻陷,发布恶意 release

上述两类仅离线私钥代码签名能堵,本次不做,作为未来可选增强记录在案(§7)。

## 3. 配置设计

### 3.1 新增配置结构(`crates/agent/src/config.rs`)

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpgradeConfig {
    #[serde(default = "default_release_repo")]
    pub release_repo_url: String,
    #[serde(default)]
    pub release_cert_spki_sha256: String, // 空=不启用 SPKI pin
}

fn default_release_repo() -> String {
    option_env!("SERVERBEE_RELEASE_REPO")
        .unwrap_or("https://github.com/ZingerLittleBee/ServerBee/releases")
        .to_string()
}
```

`AgentConfig` 新增 `#[serde(default)] pub upgrade: UpgradeConfig`。
`UpgradeConfig` 需实现 `Default`(`release_repo_url` 用 `default_release_repo()`,
`release_cert_spki_sha256` 空串)。

### 3.2 解析优先级

第 1–4 级由现有 Figment 链(`config.rs:135-139`:`/etc/serverbee/agent.toml` →
`agent.toml` → `SERVERBEE_` 前缀 env,`__` 分隔)自动处理。第 5 级 CLI 参数
`--release-repo <url>` 在 `AgentConfig::load()` 之后、`Reporter::new` 之前手动覆盖,
仿 `main.rs:45` 的 `parse_capability_args(std::env::args())` 模式,放入
`crates/agent/src/capability_policy.rs` 邻近或新模块解析。

### 3.3 URL 布局约定

`release_repo_url` 是 releases base(与 Server 端 `upgrade_release.rs` 的
`release_base_url` 同约定)。Agent 推导:

- 二进制: `{base}/download/v{version}/{asset_name}`
- 校验文件: `{base}/download/v{version}/checksums.txt`

`{base}` 去尾部 `/`。`{asset_name}` 按当前平台映射(沿用 release.yml 的 suffix 命名:
`serverbee-agent-linux-amd64` / `-linux-arm64` / `-darwin-amd64` / `-darwin-arm64` /
`-windows-amd64.exe`)。任意 HTTPS 主机只要镜像该目录结构即可自托管。

## 4. 升级数据流(`crates/agent/src/reporter.rs::perform_upgrade`)

1. 收到 `ServerMessage::Upgrade { version, .. }`,**忽略 download_url / sha256**
2. 解析 `version` 与当前版本(`env!("CARGO_PKG_VERSION")`)为 semver,
   **严格要求 `target > current`**,否则拒绝并 `emit_upgrade_failure`(防降级;
   `target == current` 与 `target < current` 均拒绝)。需新增 `semver` crate 依赖
3. 用 `release_repo_url` 自行拼接 §3.3 的两个 URL;校验 base 为 `https://` scheme
4. 用专用 HTTP client(§2.4:webpki 根 + 可选 SPKI verifier)依次拉取
   `checksums.txt` 与二进制;跟随任意重定向但每跳强制 `https`(自定义 reqwest
   redirect policy:非 https 目标即终止)
5. 从 `checksums.txt` 中按 `asset_name` 找对应哈希;计算下载内容 SHA-256;
   一致才写临时文件、设可执行位、备份旧二进制、替换、重启(沿用现有
   `tmp_path` / `backup_path` / spawn 重启逻辑)

专用 client 仅用于升级下载,**不影响**上报 WS、ping/http 探测、Server 端任何逻辑。

## 5. 协议与 Server 侧改动

- `crates/common/src/protocol.rs:429` `ServerMessage::Upgrade` 的 `download_url`
  与 `sha256` 字段**保留**(`#[serde(default)]` 已具备老/新兼容能力),
  在 doc comment 标注废弃语义:Agent 自 vNEXT 起忽略,仅 `version` 有效。
  保留字段确保新 Agent 兼容老 Server、老 Agent 兼容新 Server
- `crates/server/src/router/api/server.rs:532` `trigger_upgrade`:改为只构造
  `ServerMessage::Upgrade { version, download_url: String::new(), sha256: String::new() }`
  (废弃字段发空串占位,保持序列化兼容),不再调用 `resolve_asset`
- `crates/server/src/service/upgrade_release.rs::resolve_asset` 及 `ReleaseAsset`
  随之移除(Server 不再决定下载来源)。`latest()` / `fetch_latest()`
  保留——它仍用于 UI 展示"最新可用版本",与下载来源无关

## 6. 错误处理 / 测试 / 文档

### 6.1 错误处理

复用现有 `emit_upgrade_failure` / `emit_upgrade_progress` 与 `UpgradeStage`。
新增明确失败原因:

- 目标版本非严格递增(防降级拒绝)
- `release_repo_url` 或重定向目标非 `https://`
- SPKI pin 不匹配
- `checksums.txt` 中找不到该 asset 条目
- 下载内容哈希与 `checksums.txt` 不符

### 6.2 测试

- 防降级三态:`target > / == / < current` 分别接受 / 拒绝 / 拒绝
- SPKI verifier:命中放行、不命中拒绝、未配置时不启用走标准校验
- 配置优先级:默认 / file / env / CLI 覆盖链(沿用 config 现有测试模式)
- 老 Server 兼容:`Upgrade` 不带或带废弃 `download_url`/`sha256` 时 Agent 行为正确
- URL 推导:各平台 asset 命名 + base 去尾斜杠 + checksums URL 拼接
- 每跳强制 https 的 redirect policy:https→https 跟随,任一跳非 https 终止

### 6.3 文档

- 更新 `ENV.md` 与 `apps/docs/content/docs/{en,cn}/configuration.mdx`
  (CLAUDE.md 强制:env var 变更同步文档),新增 `[upgrade]` 段说明
- `agent.toml` 持久化写入设置权限 `0o600`(关联安全发现中
  `crates/agent/src/rebind.rs` 的 Medium:防同机其他用户篡改
  `release_repo_url` 形成本地提权路径)

## 7. 未来可选增强(本次不做)

- 离线私钥 / Sigstore keyless 代码签名:堵 §2.5 的"公共 CA 被胁迫错签"
  与"仓库账号沦陷"两类残留风险
- Agent 端自查 pinned 仓库 `releases/latest` 以彻底摆脱 Server 提供版本号

## 8. 受影响文件清单

- `crates/agent/src/config.rs` — 新增 `UpgradeConfig`
- `crates/agent/src/main.rs` — `--release-repo` CLI 覆盖
- `crates/agent/src/capability_policy.rs`(或新模块)— CLI 参数解析
- `crates/agent/src/reporter.rs` — `perform_upgrade` 重写、专用 client、防降级、SPKI verifier
- `crates/agent/Cargo.toml` — 新增 `semver` 依赖;确认 webpki-roots 特性
- `crates/common/src/protocol.rs` — `Upgrade` 字段废弃语义标注
- `crates/server/src/router/api/server.rs` — `trigger_upgrade` 简化
- `crates/server/src/service/upgrade_release.rs` — 移除 `resolve_asset` / `ReleaseAsset`
- `ENV.md`、`apps/docs/content/docs/{en,cn}/configuration.mdx` — 配置文档
