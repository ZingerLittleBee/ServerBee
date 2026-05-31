# Agent Recover End-to-End VPS 验证

在真实 Linux VPS 上完整验证 `recover agent` 流程：
浏览器 / API → server `/api/servers/{id}/recover` → 重新生成 enrollment code →
`install.sh` 在 agent.toml 已存在时刷新 `enrollment_code` 并清空旧 `token` →
agent 用新 code 重新握手成功。

适用于以下修改后做端到端回归：
- `crates/server/src/router/api/server.rs` recover endpoint
- `deploy/install.sh` 的 `install_docker_agent` / `install_binary_agent` 的 agent.toml 刷新逻辑（commit 01b6fcd9）
- 前端 `recover-agent-dialog.tsx` / `regenerate-code-dialog.tsx` 的 WS 缓存更新

`demo.serverbee.app` 上是已发布的镜像，**不能** 用来测试当前分支的代码。这套流程在 VPS 上跑你当前分支编译出来的 server + agent。

---

## 0. 前提

- 本机 macOS（Apple Silicon 已验证）。需要：
  - `docker buildx`（orbstack / Docker Desktop 都行）
  - `cargo`, `rustup target add x86_64-unknown-linux-musl`
  - `cargo-zigbuild` (`brew install cargo-zigbuild zig`) — 用它原生交叉编译，比 QEMU 模拟快 5-10 倍
  - `sshpass`（密码登录脚本化用；prod 用 key 时不需要）
- 一台干净 Linux VPS（已验证 Debian 13 trixie x86_64）。
- 一个 A 记录已指向 VPS IPv4 的域名（用于 Caddy 自动签 Let's Encrypt 证书）。
- 工作目录 = 仓库根（含 `Dockerfile.server`, `Dockerfile.agent`, `deploy/install.sh`）。

> 测试机的具体 IP/域名/凭据不入仓库。如果你有专用复用测试机，参考自己 vault 里的
> 备忘；本文示例占位符全部用变量。

变量约定（下文 shell 片段会引用）：

```bash
export VPS_IP=<your-vps-ipv4>
export VPS_USER=root
export VPS_PASS='...'                # 由人 / vault 提供，不入仓
export DOMAIN=<your-test-host.example.com>
export ACME_EMAIL=<acme-email>       # 用于 Let's Encrypt 注册
# install.sh 的 docker_image_tag 取自 GitHub release 的最新 tag（去掉前导 v）。
# 当前 main 是 v1.0.0-alpha.4，所以本地镜像必须以 `1.0.0-alpha.4` 为 tag 才能被
# install.sh 生成的 docker-compose 找到（否则 compose 会去 ghcr 拉真实 release）。
export PROD_TAG=1.0.0-alpha.4
export DEV_TAG=1.0.0-alpha.4-dev     # 给镜像加的可读 dev 别名
```

> **安全提醒**：用专用测试机；不要把生产凭据放进这套流程。

---

## 1. 本机交叉编译为 linux/amd64

### 1.1 编译前端

```bash
cd apps/web && bun install --frozen-lockfile && bun run build && cd -
# 产物：apps/web/dist/
```

### 1.2 编译 Rust 二进制（cargo-zigbuild）

```bash
cargo zigbuild --release \
  -p serverbee-server -p serverbee-agent \
  --target x86_64-unknown-linux-musl
file target/x86_64-unknown-linux-musl/release/serverbee-server
# → ELF 64-bit LSB executable, x86-64, statically linked
```

Apple Silicon M3 Max 上从 cold cache 约 4 分钟（编译 + 链接）。

> 替代方案：`docker buildx build --platform linux/amd64 -f Dockerfile .`。该方案在 macOS 上走 QEMU 模拟，cold 编译要 30-60 分钟，不推荐。

### 1.3 打包成 docker 镜像

`Dockerfile.server` / `Dockerfile.agent` 期望 `docker-bins/linux-${TARGETARCH}/serverbee-{server,agent}` 已经存在。`TARGETARCH=amd64` 由 `--platform linux/amd64` 注入。

```bash
mkdir -p docker-bins/linux-amd64
cp target/x86_64-unknown-linux-musl/release/serverbee-server docker-bins/linux-amd64/
cp target/x86_64-unknown-linux-musl/release/serverbee-agent  docker-bins/linux-amd64/

# 用 release-tag（PROD_TAG）作主 tag，让 install.sh 生成的 compose 能直接命中本地镜像
docker buildx build --platform linux/amd64 --load \
  -t ghcr.io/zingerlittlebee/serverbee-server:${PROD_TAG} \
  -t ghcr.io/zingerlittlebee/serverbee-server:${DEV_TAG} \
  -f Dockerfile.server .

docker buildx build --platform linux/amd64 --load \
  -t ghcr.io/zingerlittlebee/serverbee-agent:${PROD_TAG} \
  -t ghcr.io/zingerlittlebee/serverbee-agent:${DEV_TAG} \
  -f Dockerfile.agent .
```

> 为什么必须用 `PROD_TAG`：`install.sh` 的 `get_latest_version` 走 GitHub release API，
> 然后用 `version#v` 作 image tag。脚本顶层 `RESOLVED_VERSION=""`（[deploy/install.sh:745](../deploy/install.sh#L745)）
> 会清空任何外部 env 注入，所以无法用 env 覆盖版本。
> 同时打 `DEV_TAG` 别名只是为了 `docker images` 一眼能区分。

### 1.4 导出为 tar.gz

```bash
mkdir -p /tmp/sbee-build
docker save \
  ghcr.io/zingerlittlebee/serverbee-server:${PROD_TAG} \
  ghcr.io/zingerlittlebee/serverbee-server:${DEV_TAG} \
  ghcr.io/zingerlittlebee/serverbee-agent:${PROD_TAG} \
  ghcr.io/zingerlittlebee/serverbee-agent:${DEV_TAG} \
  | gzip > /tmp/sbee-build/serverbee-${DEV_TAG}.tar.gz
ls -lh /tmp/sbee-build/serverbee-${DEV_TAG}.tar.gz
# → 实测约 27 MB（gzip 后）
```

---

## 2. 准备 VPS（一次性）

```bash
ssh-keygen -R "$VPS_IP" 2>/dev/null || true
sshpass -p "$VPS_PASS" ssh -o StrictHostKeyChecking=accept-new $VPS_USER@$VPS_IP <<'REMOTE'
set -e

# 清掉测试机上残留的 serverbee 状态
systemctl stop serverbee-agent.service 2>/dev/null || true
systemctl disable serverbee-agent.service 2>/dev/null || true
systemctl reset-failed serverbee-agent.service 2>/dev/null || true
rm -f /etc/systemd/system/serverbee-agent.service
systemctl daemon-reload
rm -rf /opt/serverbee

# 装 Docker（Debian/Ubuntu）
if ! command -v docker >/dev/null; then
  apt-get update -qq
  apt-get install -y -qq ca-certificates curl gnupg lsb-release
  install -m 0755 -d /etc/apt/keyrings
  curl -fsSL https://download.docker.com/linux/debian/gpg \
    | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
  chmod a+r /etc/apt/keyrings/docker.gpg
  echo "deb [arch=amd64 signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $(lsb_release -cs) stable" \
    > /etc/apt/sources.list.d/docker.list
  apt-get update -qq
  apt-get install -y -qq docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
  systemctl enable --now docker
fi
docker --version
docker compose version

# Caddy 要 80/443，server 暴露 9527 给 Caddy 反代
ss -ltnp | grep -E ':(80|443|9527) ' && echo PORT_IN_USE || echo ports clean
REMOTE
```

---

## 3. scp + load 镜像

```bash
sshpass -p "$VPS_PASS" scp -o StrictHostKeyChecking=yes \
  /tmp/sbee-build/serverbee-${DEV_TAG}.tar.gz $VPS_USER@$VPS_IP:/root/

sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP \
  "gunzip -c /root/serverbee-${DEV_TAG}.tar.gz | docker load
   docker images | grep zingerlittlebee | sort"
```

预期看到 4 行（server PROD_TAG/DEV_TAG, agent PROD_TAG/DEV_TAG），且每对的 image ID 相同（同一镜像两个别名）。

---

## 4. 启动 server + Caddy 反代 HTTPS

server 容器只绑定到 `127.0.0.1:9527`，由 Caddy 在 :443 终止 TLS 后 reverse_proxy 过去。

```bash
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP <<REMOTE
set -e

mkdir -p /opt/serverbee/etc
cat > /opt/serverbee/docker-compose.server.yml <<YAML
services:
  serverbee-server:
    image: ghcr.io/zingerlittlebee/serverbee-server:${PROD_TAG}
    container_name: serverbee-server
    ports:
      - "127.0.0.1:9527:9527"
    volumes:
      - serverbee-data:/data
    environment:
      - SERVERBEE_ADMIN__USERNAME=admin
      - SERVERBEE_AUTH__SECURE_COOKIE=true
    restart: unless-stopped
volumes:
  serverbee-data:
YAML
docker compose -f /opt/serverbee/docker-compose.server.yml up -d
sleep 5

# Caddy + Let's Encrypt
if ! command -v caddy >/dev/null; then
  apt-get install -y -qq debian-keyring debian-archive-keyring apt-transport-https
  curl -1sLf "https://dl.cloudsmith.io/public/caddy/stable/gpg.key" \
    | gpg --dearmor --batch --yes -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
  curl -1sLf "https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt" \
    > /etc/apt/sources.list.d/caddy-stable.list
  chmod o+r /usr/share/keyrings/caddy-stable-archive-keyring.gpg /etc/apt/sources.list.d/caddy-stable.list
  apt-get update -qq && apt-get install -y -qq caddy
fi
cat > /etc/caddy/Caddyfile <<CADDY
{
  email ${ACME_EMAIL}
}
${DOMAIN} {
  reverse_proxy 127.0.0.1:9527
}
CADDY
systemctl restart caddy
sleep 8
journalctl -u caddy --no-pager -n 20

# 一次性抓 admin 初始密码
docker logs serverbee-server 2>&1 | awk '/FIRST-RUN ADMIN CREDENTIALS/,/=========/' | tail -20
REMOTE
```

预期：

- `caddy ... certificate obtained successfully` — Let's Encrypt 拿证成功。
- 日志里能看到 `Username: admin` + `Password: <43位>`，复制下来到下一步用。

### 4.1 外部验证 HTTPS

```bash
curl -fsS -I https://$DOMAIN/healthz | head -3   # 期望 HTTP/2 200
curl -fsS https://$DOMAIN/healthz                # 期望 ok
```

---

## 5. 完成 onboarding + 创建 server 实体

新装的 server 强制首次登录改密：`must_change_password=true` 的用户只能调
`POST /api/auth/onboarding`、`GET /api/auth/me`、`POST /api/auth/logout`
（白名单见 [crates/server/src/middleware/auth.rs](../../crates/server/src/middleware/auth.rs)
的 `is_onboarding_whitelisted`）。直接调 `PUT /api/auth/password` 会被中间件拦住。

```bash
export INIT_PASS='...从 docker logs 拷出来...'
export NEW_PASS='<strong-test-password>'  # >=8 位，符合 server 密码策略；不要复用生产密码

# 5.1 登录拿 session cookie
rm -f /tmp/sb.cookies
curl -sS -c /tmp/sb.cookies -X POST https://$DOMAIN/api/auth/login \
  -H 'Content-Type: application/json' \
  -d "{\"username\":\"admin\",\"password\":\"$INIT_PASS\"}"
# → must_change_password: true

# 5.2 走 onboarding 改密
curl -sS -b /tmp/sb.cookies -c /tmp/sb.cookies -X POST https://$DOMAIN/api/auth/onboarding \
  -H 'Content-Type: application/json' \
  -d "{\"new_password\":\"$NEW_PASS\"}"
# → {"data":"ok"}

# 5.3 创建一台 server，拿初次 enrollment code
curl -sS -b /tmp/sb.cookies -X POST https://$DOMAIN/api/servers \
  -H 'Content-Type: application/json' \
  -d '{"name":"vps-recover-test"}' | tee /tmp/sb.server.json | jq .

export SERVER_ID=$(jq -r '.data.server_id' /tmp/sb.server.json)
export INIT_CODE=$(jq -r '.data.enrollment.code' /tmp/sb.server.json)
echo "SERVER_ID=$SERVER_ID  INIT_CODE=$INIT_CODE"
```

---

## 6. 首次 install.sh 装 agent

把当前分支的 install.sh 推上去，docker 模式装：

```bash
sshpass -p "$VPS_PASS" scp -o StrictHostKeyChecking=yes \
  deploy/install.sh $VPS_USER@$VPS_IP:/root/install.sh

sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP "set -e
bash /root/install.sh install agent --method docker \
  --server-url https://${DOMAIN} \
  --enrollment-code ${INIT_CODE} \
  --yes
cat /opt/serverbee/etc/agent.toml
docker inspect serverbee-agent --format '{{.Image}}'"
```

关键校验：

- 控制台显示 `[INFO] Created /opt/serverbee/etc/agent.toml`（首次 = 走 `if` 分支）
- `agent.toml` 里 `server_url=...`, `enrollment_code=$INIT_CODE`，没有 `token` 行
- `docker inspect ...Image` 输出的 sha256 与 `docker images | grep ghcr.io/zingerlittlebee/serverbee-agent:${PROD_TAG}` 的 ID 一致 → 确认在跑你的本地编译，而不是从 ghcr 拉的发布镜像
- VPS 上 `ss -tnp | grep :443` 可看到 `serverbee-agent` ESTAB 到 :443 的连接
- server REST `/api/servers/$SERVER_ID` 期望 `has_token=true`、`outstanding_enrollment=null`、`agent_version` / `cpu_name` 等字段被 agent 上报填上
- server logs 出现 `Agent <id> connected from ...`

> `online` 字段在 REST 里恒为 `null` —— 它是 WS push 才会刷的运行时态，不出现在 `/api/servers` 响应里。判定 agent 实际在线看 server 日志的 `connected` 行或 TCP 连接。

---

## 7. ⭐ 核心修复点验证：recover + 二次 install.sh

这是真正要回归的 bug。先让 server 撤回 token + 发新 code，再让 `install.sh` 走 agent.toml 已存在的 `else` 分支。

### 7.1 触发 recover（revoke_immediately=true）

```bash
curl -sS -b /tmp/sb.cookies -X POST "https://$DOMAIN/api/servers/$SERVER_ID/recover" \
  -H 'Content-Type: application/json' \
  -d '{"revoke_immediately":true}' | tee /tmp/sb.recover.json | jq .

export NEW_CODE=$(jq -r '.data.enrollment.code' /tmp/sb.recover.json)
curl -sS -b /tmp/sb.cookies "https://$DOMAIN/api/servers/$SERVER_ID" \
  | jq '.data | {has_token, outstanding_enrollment}'
```

预期：

- `has_token=false`（旧 token 失效）
- `outstanding_enrollment.code_prefix = "..."` 与 `NEW_CODE` 前 8 位一致

VPS 上 server 日志会出现 agent 用旧 token 反复重连失败的 `Agent WS unauthorized ... invalid token (source=query, prefix=<old_prefix>)`。这就是修复前会卡死循环的状态。

### 7.2 抓 agent.toml 「before」状态

```bash
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP "cat /opt/serverbee/etc/agent.toml"
```

预期看到：旧 `enrollment_code`、`token = "<旧 token>"`（agent 注册成功后 toml_set 写进去的）。

### 7.3 走「重装」流程：uninstall + install 同一份 code

install.sh 的 `cmd_install` 会先检查 meta 文件，若有就拒装。**recover 的预期 UX 是
先 `uninstall agent --yes`（保留 agent.toml）再 `install agent ...` 同样命令但带新 code**。
`uninstall` 不带 `--purge` 时只删 container 和 systemd 单元，**保留 agent.toml**，
这样下一次 install 才会命中 `else` 分支去刷新而不是新建。

```bash
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP "set -e
bash /root/install.sh uninstall agent --yes

# agent.toml 仍在（这是 else 分支触发的前提）
cat /opt/serverbee/etc/agent.toml

bash /root/install.sh install agent --method docker \
  --server-url https://${DOMAIN} \
  --enrollment-code ${NEW_CODE} --yes

# 关键：agent.toml 刷新后的内容
cat /opt/serverbee/etc/agent.toml"
```

### 7.4 修复有效的判定

`cat /opt/serverbee/etc/agent.toml` 必须**全部**满足：

| 字段 | 期望值 |
| --- | --- |
| `server_url` | `"https://${DOMAIN}"`（重写） |
| `enrollment_code` | **新 code**（`NEW_CODE`，而不是 7.2 抓到的旧 code） |
| `token` | **空字符串** `""`（被 install.sh 主动清空） |
| `[collector]` section | 原样保留 |

控制台 install.sh 应该打印 `[INFO] /opt/serverbee/etc/agent.toml exists — refreshing server_url, enrollment_code, clearing token` —— 这就是 [deploy/install.sh:1535](../../deploy/install.sh#L1535)（docker 路径）/ [:1665](../../deploy/install.sh#L1665)（binary 路径） 的 `else` 分支。

> 修复前的 bug：旧 `else` 分支只 `warn "agent.toml already exists, not overwriting"`，
> 上面三行 **全部** 保持旧值。agent 重启会用作废 token 反复打 server，server 日志狂刷
> `invalid token (source=query, prefix=...)`，恢复永远不发生。

### 7.5 server 端确认重连

```bash
sleep 5
curl -sS -b /tmp/sb.cookies "https://$DOMAIN/api/servers/$SERVER_ID" \
  | jq '.data | {has_token, outstanding_enrollment, agent_version, updated_at}'

sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP \
  "docker logs serverbee-server 2>&1 | grep -E 'Agent.*(connected|unauthorized|disconnect)' | tail -10"
```

预期 server REST：`has_token=true`、`outstanding_enrollment=null`、`updated_at` 是 install.sh 之后的时间戳。

预期 server 日志会出现完整的恢复时间线：

```
... Agent <id> connected ...                                  # 6 节首装
... Agent <id> disconnected                                   # 7.1 recover 撤 token
... Agent WS unauthorized ... invalid token (prefix=<旧>)     # 旧 token 反复重连失败
... Agent <id> connected from ...                             # 7.3 之后用新 code 重新握手
```

---

## 8. 一次实际跑通的样本数据（M3 Max + Debian 13 VPS）

| 步骤 | 实际耗时 |
| --- | --- |
| `cargo zigbuild` cold | ~4 min |
| `docker buildx` 两个镜像（COPY 已编译二进制） | < 1 s |
| `docker save | gzip` 两镜像 | ~5 s（合计约 27 MB gzipped） |
| `scp` 镜像 tarball 到 VPS | ~15 s |
| `docker load` on VPS | ~10 s |
| `docker compose up -d` server | ~5 s |
| Caddy 装 + Let's Encrypt 签证书 | ~15 s |
| install.sh 首装 agent → 上线 | ~10 s |
| recover + uninstall + reinstall + 重连 | ~25 s |

总流程从 cold cargo cache 起约 8-10 分钟。命中 cache 复跑约 2-3 分钟。

---

## 9. 清理

测试机复用（保留镜像与压缩包，仅清服务和域名配置）：

```bash
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP <<'REMOTE'
docker compose -f /opt/serverbee/docker-compose.agent.yml down -v 2>/dev/null || true
docker compose -f /opt/serverbee/docker-compose.server.yml down -v 2>/dev/null || true
rm -rf /opt/serverbee
systemctl stop caddy && rm -f /etc/caddy/Caddyfile
REMOTE
```

完全清场（含镜像）：再加
`docker rmi -f $(docker images -q 'ghcr.io/zingerlittlebee/serverbee-*')`。

---

## 10. 失败排查表

| 症状 | 排查点 |
| --- | --- |
| `[ERROR] Failed to get latest version from GitHub` | install.sh 联网拿不到 release tag；查 VPS DNS / 出网；或直接 `RESOLVED_VERSION` 注入是无效的（脚本第 745 行会清空） |
| Caddy 拿不到证书 | `journalctl -u caddy -n 50`；80/443 防火墙；DNS A 记录未生效；存在错指的 AAAA |
| HTTPS 200 但 server 容器 unhealthy | `docker logs serverbee-server` 看是否数据库 migration 卡死 |
| `MUST_CHANGE_PASSWORD` 错误 | 先调 `POST /api/auth/onboarding`，**不要** 调 `PUT /api/auth/password` |
| install.sh 报 `serverbee-agent is already installed (...). Use 'upgrade' to update.` | recover 流程要先 `uninstall agent --yes` 再 `install ... --enrollment-code <new>`；`uninstall` 不带 `--purge` 会保留 agent.toml，正是 else 分支触发条件 |
| compose 去拉 ghcr 上的真实 release 而不是本地镜像 | 你的本地镜像 tag 不是 `PROD_TAG`（必须 = release 版本字符串去掉 `v`）。重新 `docker tag` 后 `compose up -d` 不会再 pull |
| agent 容器跑起来但 `docker logs` 空 | 正常 —— Rust 默认 `RUST_LOG` 没设，agent 静默运行；判断在线看 server 日志的 `connected` 行或 `ss -tnp | grep :443` |
| 7.4 token 没清空 | `deploy/install.sh` HEAD 不含 fix `01b6fcd9`；或你 install.sh 的 else 分支被某个 patch 改回 `warn ... not overwriting` |
| 7.4 enrollment_code 没换 | 同上；或者 `toml_set` 自身坏了，看 `deploy/install.sh:2747` 附近 |
| outstanding_enrollment 没刷新到前端列表 | server router/recover 没落库 / 前端缓存补丁没生效；查 `apps/web/src/components/server/recover-agent-dialog.tsx` 是否用 `setQueryData` 补丁 `['servers']` 而不是 invalidate |

---

## 11. POSIX sh + OpenRC 安装路径回归（Alpine）

> 改了 `deploy/install.sh` 的 init 抽象（`svc_*` 分发、`create_openrc_service_*`、
> `create_systemd_unit_*`）、sha256 校验、doas/sudo re-exec、logrotate、i18n 之后，
> 必须在 **真正的 OpenRC** 上跑一遍——静态 `shellcheck --shell=dash` / `dash -n`
> 只能保证可移植语法，证明不了 `rc-service` / `supervise-daemon` / `rc-update`
> 的运行时行为。给的测试机若是 systemd（如 Debian/Ubuntu），用 **VPS 上的
> privileged Alpine docker 容器** 起一个 OpenRC PID 1 即可，不必单独开 Alpine VM。

### 11.1 在 systemd 测试机上起一个 OpenRC 容器

```bash
# VPS 上（需要 docker）。Dockerfile 见下。
cat > /root/Dockerfile.alpine-openrc <<'EOF'
FROM alpine:3.20
RUN apk add --no-cache openrc curl ca-certificates procps
RUN echo 'rc_sys="docker"' >> /etc/rc.conf \
 && sed -i 's/^tty/# tty/g' /etc/inittab \
 && rm -f /etc/init.d/hwdrivers /etc/init.d/machine-id 2>/dev/null || true
CMD ["/sbin/init"]
EOF
docker build -t sb-alpine-openrc -f /root/Dockerfile.alpine-openrc /root

docker rm -f sb-alpine 2>/dev/null
docker run -d --name sb-alpine --privileged \
  --tmpfs /run --tmpfs /run/lock \
  -v /sys/fs/cgroup:/sys/fs/cgroup:rw \
  sb-alpine-openrc
sleep 6

# OpenRC 真起来了的判据：default runlevel + softlevel + 三个二进制就位
docker exec sb-alpine rc-status            # → Runlevel: default
docker exec sb-alpine sh -c 'cat /run/openrc/softlevel'   # → default
docker exec sb-alpine sh -c 'command -v rc-service supervise-daemon rc-update; ls /sbin/openrc-run'
```

把当前分支的 install.sh 拷进容器：`docker cp deploy/install.sh sb-alpine:/root/install.sh`。

### 11.2 逐项验证（实测全过，Alpine 3.20 / x86_64 / busybox ash）

| # | 命令 | 期望 |
| --- | --- | --- |
| 1 | `docker exec sb-alpine sh -n /root/install.sh` | ash 语法干净（无输出） |
| 2 | `sh /root/install.sh server --method binary -y` | `INIT=openrc` 自动命中；下载 musl 二进制；`Starting serverbee-server ... [ ok ]`；`rc-update` 加进 default |
| 3 | `rc-service serverbee-server status` / `curl /healthz` | `status: started` / `ok` |
| 4 | `cat /etc/init.d/serverbee-server` | `#!/sbin/openrc-run`、`supervisor=supervise-daemon`、`depend(){ need net }`、`start_pre()` source env、`output_log`/`error_log` 指向 `/var/log/serverbee-server.log` |
| 5 | `cat /etc/logrotate.d/serverbee-server` | weekly / rotate 4 / copytruncate |
| 6 | 装 agent（先 onboarding 建 server 拿 code，见第 5 节，但 BASE 用 `http://127.0.0.1:9527`）| `Capabilities: ...(default)`、`Created agent.toml`、`Starting serverbee-agent ... [ ok ]` |
| 7 | `curl -b cj /api/servers` | agent 上线：`os":"Alpine Linux ..."`、`capabilities":1852`、`has_token":true`、`protocol_version":4` |
| 8 | `cat /etc/init.d/serverbee-agent` | 含 **`respawn_max=5`** + **`respawn_period=300`**（systemd `StartLimitBurst/IntervalSec` 的 OpenRC 等价物） |
| 9 | `ps aux \| grep supervise-daemon` | 实际带 `--respawn-delay 5 --respawn-max 5 --respawn-period 300` |
| 10 | `serverbee status` | agent + server 都 `active (running)` |
| 11 | `serverbee restart` | server / agent 都干净 stop+start，healthz 回 200（连跑数次不偶发） |
| 12 | `serverbee config set collector.interval 5 -y` | 改 `agent.toml`，打印 diff |
| 13 | `serverbee env set COLLECTOR__INTERVAL 5 -y` | 写 `/opt/serverbee/etc/serverbee-agent.env`（`SERVERBEE_` 前缀），并自动重启 agent |
| 14 | `serverbee uninstall agent -y`（非 purge） | 删 init 脚本 / logrotate / log / env，`rc-update del`，**保留 agent.toml** |
| 15 | `serverbee uninstall server --purge -y` | 全清，且 `/opt/serverbee` base dir 消失（commit 4b1b2b0c 的修复在 OpenRC 同样生效） |

### 11.3 ⭐ exit-78（永久 enrollment 失败）的 respawn 上限验证

OpenRC 没有 systemd `RestartPreventExitStatus=78` 的等价物。为避免坏 enrollment code
导致 `supervise-daemon` 每 5s 无限重启，agent init 脚本带 `respawn_max=5` /
`respawn_period=300`——300s 内超过 5 次 respawn 就放弃，逼近 systemd 行为。验证：

```bash
# server 在线 + agent.toml 是 bogus code（uninstall 后重装带假 code，token 会被清空）
sh /root/install.sh agent --server-url http://127.0.0.1:9527 \
  --enrollment-code BOGUS-INVALID-xxxxxxxxxxxxxxxxxxxxxxxxxxxx -y
rc-service serverbee-agent restart
# 轮询 supervise-daemon 进程数；约 25-30s（≈5×respawn_delay）后应归 0
while sleep 5; do pgrep -f 'supervise-daemon serverbee-agent' | wc -l; done
```

预期：`supervise-daemon` 在约 5 次后退出（进程数 0），`rc-service serverbee-agent
status` → `stopped`，`/var/log/serverbee-agent.log` 里正好 **5 条**
`Permanent registration failure: HTTP 401`。**不是** 无限循环。

> 顺带说明：healthy agent 在默认日志级别下不往 stdout 写，所以 `output_log` 平时是
> 空文件（systemd 下 `journalctl` 同样为空）——这是 agent 二进制行为，不是 OpenRC
> 日志管道坏了；exit-78 这条恰好证明了 stderr 重定向是通的（错误被捕获进日志）。

### 11.4 在 systemd 上的对照

同样的 1-15 项在 systemd 测试机（Debian 13 / dash）上已全过（ALL_PASS）；
`serverbee env set` 写 systemd drop-in override，`od -c` 字节级确认；
`svc_logs` 走 `journalctl -u serverbee-*`。两套 init 共用同一份 `svc_*` 分发，
唯一差异是上面 11.2 #4/#8 的 unit 文件格式。
