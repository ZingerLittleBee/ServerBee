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
  自定义 `ServerCertVerifier` 必须**包裹(委托)**标准 `WebPkiServerVerifier`:
  先调用其完成完整证书链 + 主机名 + 时间校验,**仅在其返回成功后**再追加比对
  leaf 证书 SPKI 的 SHA-256,任一步失败即拒绝。**严禁**自行实现链校验或在自定义
  verifier 里直接返回成功(常见错误:把标准链校验整个替换掉,反而更不安全)。
  主要服务于自托管者锁定自己镜像的证书。**不对 github.com 默认源使用**
  (GitHub leaf + Fastly CDN 证书频繁轮换,pin 会随轮换失效)。

### 2.5 已知残留风险(spec 明确接受)

- 合法公共 CA 被胁迫 / 被攻陷,专门为目标主机错签证书(国家级 / CA 级攻击)
- pinned 仓库 / GitHub 账号本身被攻陷,发布恶意 release
- 被攻陷 Server 把 `version` 指向 pinned 仓库里**真实存在的预发布版**(如
  `1.2.0-rc.1`):防降级用 `semver` 严格大于,`1.2.0-rc.1 > 当前稳定版` 成立,
  会被接受。经决策**接受此残留**(危害较低:目标版本必须本身已存在于可信
  pinned 仓库;不接受预发布版的 opt-in 留作未来增强 §7)

前两类仅离线私钥代码签名能堵,本次不做,作为未来可选增强记录在案(§7)。

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

**`release_cert_spki_sha256` 格式规范**:64 字符小写 hex(SHA-256 of
SubjectPublicKeyInfo DER,无 `:`/空白/`0x` 前缀);加载时规范化为去首尾空白
并转小写后校验。空串=不启用。**非空但格式非法(长度≠64 或含非 hex 字符)→
Agent 启动即失败(fail-fast)**,而非延迟到升级时才报错——避免运维以为
pin 生效实则配错。获取方式(写入配置文档,**必须直出纯 hex**——
`openssl dgst -sha256` 默认输出带 `SHA2-256(stdin)= ` 前缀,整行复制会触发
fail-fast,故用 `-r` + `awk` 去前缀):

```
openssl x509 -in cert.pem -pubkey -noout \
  | openssl pkey -pubin -outform der \
  | openssl dgst -sha256 -r | awk '{print $1}'
```

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

- `crates/common/src/protocol.rs:429` `ServerMessage::Upgrade`:当前实际只有
  `job_id` 带 `#[serde(default)]`,`version/download_url/sha256` **均无**。
  本次给 `download_url`、`sha256` **显式加 `#[serde(default)]`**,并在 doc
  comment 标注废弃语义:Agent 自 vNEXT 起忽略这两个字段,仅 `version` 有效
- `crates/server/src/router/api/server.rs:532` `trigger_upgrade`:改为只构造
  `ServerMessage::Upgrade { version, download_url: String::new(), sha256: String::new() }`
  (废弃字段发空串占位),不再调用 `resolve_asset`
- `crates/server/src/service/upgrade_release.rs::resolve_asset` 及 `ReleaseAsset`
  移除(Server 不再决定下载来源)。`latest()` / `fetch_latest()` 保留(见 §5.2)

### 5.1 破坏性变更与迁移(已决策:不做向后兼容)

新 Server 只发 `version`,`download_url`/`sha256` 发空串。**存量旧 Agent
收到空 `download_url` 会在 `reporter.rs:1906` 因非 `https://` 立即失败,
无法自动升级。** 这是经决策**有意接受的破坏性变更**:

- 升级到含本变更的 Server 后,所有低于 vNEXT 的存量 Agent **必须人工重装一次**
  (重新跑安装脚本 / 部署新二进制),之后才进入 pinned-source 自升级体系
- 必须在 CHANGELOG、release notes、升级文档**显著标注**该一次性人工迁移要求
- 旧 Agent 在被手动替换前继续运行旧(有漏洞)升级路径——但其只会响应
  其已连接 Server 的指令,风险等同其本就运行的旧代码

### 5.2 UI "最新版本" 与 Agent pinned 源可能不一致(Finding 4)

Server 的 `latest()` 读 **Server 配置的** release 源,Agent 下载读 **Agent 自己
pinned 的** `release_repo_url`,二者可被配成不同源。后果:UI 可能展示一个
Agent 镜像里不存在的版本,触发升级后 Agent 拉取 404 而失败(非安全问题,
是 UX 误导)。处理:

- UI 文案改为 advisory("最新发布版本(来自 Server 配置源,Agent 实际下载源
  以其本地配置为准)"),不承诺与 Agent 下载源一致
- 配置文档明确建议:除非有意分流,Server 与 Agent 的 release 源应配同一仓库

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
  (含一条:`target` 为预发布版且 `> current` 时按 §2.5 决策**接受**,锁定该行为)
- SPKI verifier:命中放行、不命中拒绝、未配置时不启用走标准校验;
  非法 `release_cert_spki_sha256`(长度≠64 / 非 hex)→ 启动 fail-fast
- 配置优先级:默认 / file / env / CLI 覆盖链(沿用 config 现有测试模式)
- 协议反序列化:`Upgrade` 省略 `download_url`/`sha256` 时凭 `#[serde(default)]`
  正常解析;Agent 收到空串时**忽略**而非当 URL 用(防回归到 §5.1 失败路径)
- URL 推导:各平台 asset 命名 + base 去尾斜杠 + checksums URL 拼接
- 每跳强制 https 的 redirect policy:https→https 跟随,任一跳非 https 终止

### 6.3 文档

- 更新 `ENV.md` 与 `apps/docs/content/docs/{en,cn}/configuration.mdx`
  (CLAUDE.md 强制:env var 变更同步文档),新增 `[upgrade]` 段说明,
  含 `release_cert_spki_sha256` 获取命令(§3.1)与 Server/Agent 同源建议(§5.2)
- **CHANGELOG + release notes 显著标注 §5.1 破坏性变更**:存量旧 Agent
  须人工重装一次,否则无法自动升级
- `agent.toml` 持久化写入设置权限 `0o600`(关联安全发现中
  `crates/agent/src/rebind.rs` 的 Medium:防同机其他用户篡改
  `release_repo_url` 形成本地提权路径)
- **启动时配置权限检查(Unix)**:对实际加载的 `agent.toml`(`config.rs`
  Figment 实际命中的那个路径)做权限检查,若 group/world-writable
  则输出**醒目 warn 日志但继续运行**(不 fail-fast,避免 brick 存量
  0644 部署,不与 §5.1 破坏性变更叠加)。仅写权限策略不够——既有写入
  加固不覆盖"启动时读取一个早已 0644 的配置文件"这一面

## 7. 未来可选增强(本次不做)

- 离线私钥 / Sigstore keyless 代码签名:堵 §2.5 的"公共 CA 被胁迫错签"
  与"仓库账号沦陷"两类残留风险
- Agent 端自查 pinned 仓库 `releases/latest` 以彻底摆脱 Server 提供版本号
- 可配置 opt-in:拒绝预发布 / 带 build metadata 的目标版本(堵 §2.5
  第三类残留——被攻陷 Server 推可信仓库里真实存在的 rc/beta 版)

## 8. 受影响文件清单

- `crates/agent/src/config.rs` — 新增 `UpgradeConfig`
- `crates/agent/src/main.rs` — `--release-repo` CLI 覆盖
- `crates/agent/src/capability_policy.rs`(或新模块)— CLI 参数解析
- `crates/agent/src/reporter.rs` — `perform_upgrade` 重写、专用 client、防降级、SPKI verifier
- `crates/agent/Cargo.toml` — 见 §8.1 依赖决策
- `crates/common/src/protocol.rs` — `Upgrade` 字段废弃语义标注
- `crates/server/src/router/api/server.rs` — `trigger_upgrade` 简化
- `crates/server/src/service/upgrade_release.rs` — 移除 `resolve_asset` / `ReleaseAsset`
- `ENV.md`、`apps/docs/content/docs/{en,cn}/configuration.mdx` — 配置文档

### 8.1 依赖决策(实现计划须先定稿,不得偷传递依赖)

`crates/agent/Cargo.toml` 现状:`reqwest{rustls-tls}` / `sha2` / `hex` / `url`,
**无 semver、无 x509 解析器、reqwest 未显式启用 webpki-roots**
(`tokio-tungstenite` 已用 `rustls-tls-webpki-roots`,但那是另一个 client)。
需新增 / 确认:

1. **`semver`**(新增):防降级版本比较
2. **reqwest 根证书库**:确认 reqwest 的 `rustls-tls` 在本项目下解析到的
   根来源;§2.4 要求升级下载 client 用 **webpki-roots**(忽略 OS 信任库)。
   实现计划须明确:改用 `rustls-tls-webpki-roots` 特性,或 `use_preconfigured_tls`
   注入以 `webpki-roots` 构建的 `ClientConfig`
3. **SPKI 提取依赖**(开放,实现计划须研究并锁定):自定义
   `ServerCertVerifier` 拿到的是 leaf `CertificateDer`,需从中取
   SubjectPublicKeyInfo 的 DER 再 SHA-256。候选:`x509-parser` 或
   RustCrypto `x509-cert`(纯 Rust)。**不得依赖未声明的传递依赖**;
   计划阶段必须确定选型并评估体积/维护

> 该小节存在的目的:这些依赖不定稿,实现会卡住。计划阶段先做依赖 spike。
