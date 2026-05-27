# 完整部署流程 End-to-End VPS 验证

通过 `deploy/install.sh` 在真实 Linux VPS 上把当前分支 **完整** 跑一遍：

1. `install server --method docker --domain <host>` —— 验证 install.sh 自动起 server 容器 + 安装/配置 Caddy + Let's Encrypt + DNS 检查 + HTTPS 验证
2. `install agent --method docker` —— 验证 docker 模式的 agent 装机与连接
3. `install agent --method binary` —— 验证 binary/systemd 模式的 agent 装机与连接（**adopt mode** 用我们本地编译的二进制，而不是去 ghcr 拉真实 release）
4. `uninstall {agent,server} --purge` —— 验证彻底清场

适用于：
- 改 `deploy/install.sh` 的任何主路径（install_*、uninstall_*、cmd_domain、Caddyfile 生成）
- 改 server 启动 / 数据库迁移 / OnboardingResponse
- 升级 docker base 镜像

Recover-only 的窄回归用 [agent-recover-e2e.md](agent-recover-e2e.md)；本文是它的超集。

---

## 0. 前提

同 [agent-recover-e2e.md §0](agent-recover-e2e.md#0-前提)：本机 macOS + cargo-zigbuild + sshpass；VPS Debian/Ubuntu x86_64；域名 A 记录已指向 VPS。

> 测试机的 IP/域名/凭据不入仓库。下面所有命令都用变量占位符引用，按你自己 vault
> 里的实际值导出后再跑。

变量约定（下文 shell 片段会引用）：

```bash
export VPS_IP=<your-vps-ipv4>
export VPS_USER=root
export VPS_PASS='...'                      # 由人 / vault 提供，不入仓
export DOMAIN=<your-test-host.example.com>
export ACME_EMAIL=<acme-email>             # 用于 Let's Encrypt 注册
export PROD_TAG=1.0.0-alpha.4              # = GitHub release 最新 tag 去掉 v 前缀
export DEV_TAG=1.0.0-alpha.4-dev
```

---

## 1. 本机构建 + 推镜像 / 二进制到 VPS

按 [agent-recover-e2e.md §1–§3](agent-recover-e2e.md#1-本机交叉编译为-linuxamd64) 跑一遍：编译 web、`cargo zigbuild` 出 `target/x86_64-unknown-linux-musl/release/serverbee-{server,agent}`、打 docker 镜像（tag 必须等于 `PROD_TAG`）、`docker save | gzip > /tmp/sbee-build/serverbee-${DEV_TAG}.tar.gz`、scp 到 VPS、`docker load`。

到这里 VPS 上应有：

```
ghcr.io/zingerlittlebee/serverbee-server:1.0.0-alpha.4       <ID>
ghcr.io/zingerlittlebee/serverbee-server:1.0.0-alpha.4-dev   <same ID>
ghcr.io/zingerlittlebee/serverbee-agent:1.0.0-alpha.4        <ID>
ghcr.io/zingerlittlebee/serverbee-agent:1.0.0-alpha.4-dev    <same ID>
```

把当前分支的 install.sh 也推上去（每次都推，确保你测的是 HEAD）：

```bash
sshpass -p "$VPS_PASS" scp -o StrictHostKeyChecking=yes \
  deploy/install.sh $VPS_USER@$VPS_IP:/root/install.sh
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP "chmod +x /root/install.sh"
```

---

## 2. 清场（如果 VPS 之前装过）

```bash
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP <<'REMOTE'
set -e
bash /root/install.sh uninstall agent --yes --purge 2>/dev/null || true
bash /root/install.sh uninstall server --yes --purge 2>/dev/null || true
systemctl stop caddy 2>/dev/null || true
rm -f /etc/caddy/Caddyfile
rm -rf /opt/serverbee /usr/local/bin/serverbee
echo "=== verify clean ==="
ls /opt/serverbee 2>/dev/null || echo "no /opt/serverbee"
docker ps -a | grep serverbee || echo "no serverbee containers"
REMOTE
```

> `uninstall ... --purge` 会删 docker 镜像 / volume，但保留 Caddy 软件包本身（Caddy 不属于 ServerBee）。Caddyfile 我们手动清。
> 如果你刚才 `--purge` 了 server 镜像，记得把 `${DEV_TAG}` 那份镜像重新 `docker load` 回来（agent 同理）。

---

## 3. ⭐ install.sh install server（含 Caddy + HTTPS 自动化）

这是上一份 recover runbook 没覆盖的主路径。命令：

```bash
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP \
  "bash /root/install.sh install server \
     --method docker \
     --domain ${DOMAIN} \
     --email ${ACME_EMAIL} \
     --yes"
```

install.sh 会顺序做：

| 阶段 | 期望输出（截选） |
| --- | --- |
| 写 `/opt/serverbee/etc/server.toml` | `[INFO] Created /opt/serverbee/etc/server.toml` |
| 写 `/opt/serverbee/docker-compose.server.yml`（image=PROD_TAG） | `[INFO] Generated ... docker-compose.server.yml` |
| `docker compose up -d` | `Container serverbee-server Started` |
| 打印**一次性** admin 密码 banner | `Username: admin` / `Password: <43位>`，下一节用 |
| 装 `serverbee` 管理 CLI 到 `/usr/local/bin` | `[INFO] Management CLI installed: serverbee` |
| DNS 检查 | `[INFO] DNS check passed: ${DOMAIN} resolves to this server.` |
| 装 Caddy（已装则跳过）+ 写 Caddyfile + restart | `[INFO] Caddy is already installed` / `Configured /etc/caddy/Caddyfile for ${DOMAIN}` |
| **重启 server 容器**（让它知道现在在反代后面） | `Container serverbee-server Recreated` |
| Verify HTTPS endpoint | `[INFO] Verifying HTTPS endpoint...` `ServerBee HTTPS domain configured successfully!` |

外部 sanity check：

```bash
curl -fsS -I https://$DOMAIN/healthz | head -3   # HTTP/2 200
curl -fsS https://$DOMAIN/healthz                # ok
```

### 3.1 关键产物快照（实测样本）

```
/opt/serverbee/
├── docker-compose.server.yml     # image=ghcr.io/zingerlittlebee/serverbee-server:1.0.0-alpha.4
├── etc/
│   └── server.toml               # [server] data_dir = "/data"
/etc/caddy/Caddyfile              # email + ${DOMAIN} { reverse_proxy 127.0.0.1:9527 }
/usr/local/bin/serverbee          # management CLI symlink to /root/install.sh
```

---

## 4. Onboarding + 创建 server 实体

新装 server 强制改密，必须先调 `POST /api/auth/onboarding`（直接调 `PUT /api/auth/password` 会被中间件 `MUST_CHANGE_PASSWORD` 拦）。

```bash
export INIT_PASS='...从 docker logs 拷出来...'    # install.sh 输出里的一次性密码
export NEW_PASS='<strong-test-password>'    # >=8 位，符合 server 密码策略；不要复用生产密码

rm -f /tmp/sb.cookies
curl -sS -c /tmp/sb.cookies -X POST https://$DOMAIN/api/auth/login \
  -H 'Content-Type: application/json' \
  -d "{\"username\":\"admin\",\"password\":\"$INIT_PASS\"}"

curl -sS -b /tmp/sb.cookies -c /tmp/sb.cookies -X POST https://$DOMAIN/api/auth/onboarding \
  -H 'Content-Type: application/json' \
  -d "{\"new_password\":\"$NEW_PASS\"}"
# → {"data":"ok"}

curl -sS -b /tmp/sb.cookies -X POST https://$DOMAIN/api/servers \
  -H 'Content-Type: application/json' \
  -d '{"name":"vps-fulldeploy-test"}' | tee /tmp/sb.server.json | jq .

export SERVER_ID=$(jq -r '.data.server_id' /tmp/sb.server.json)
export INIT_CODE=$(jq -r '.data.enrollment.code' /tmp/sb.server.json)
echo "SERVER_ID=$SERVER_ID  INIT_CODE=$INIT_CODE"
```

---

## 5. install.sh install agent —— **docker** 模式

```bash
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP \
  "bash /root/install.sh install agent \
     --method docker \
     --server-url https://${DOMAIN} \
     --enrollment-code ${INIT_CODE} \
     --yes"
```

### 5.1 校验

```bash
# 容器跑的是我们的本地镜像
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP \
  "docker inspect serverbee-agent --format '{{.Image}}'"
# 期望 sha256 = docker images | grep serverbee-agent:${PROD_TAG} 的 ID

# server 看到 agent 已注册并上报硬件信息
curl -sS -b /tmp/sb.cookies "https://$DOMAIN/api/servers/$SERVER_ID" \
  | jq '.data | {has_token, agent_version, cpu_name, mem_total, outstanding_enrollment}'
# 期望 has_token=true, outstanding_enrollment=null, cpu_name/mem_total 都填上

# server 容器日志
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP \
  "docker logs serverbee-server 2>&1 | grep 'Agent.*connected' | tail -3"
```

---

## 6. install.sh install agent —— **binary** 模式（systemd unit）

binary 模式默认会去 GitHub Releases 下 `serverbee-agent-linux-amd64`。要测当前分支代码，
利用 install.sh 的 **adopt mode**：[deploy/install.sh:1502](../../deploy/install.sh#L1502)
`if [ -f "${INSTALL_DIR}/serverbee-agent" ]; then warn ... "skipping download (adopting existing)"`。
预先把本地编译好的二进制放到 `/opt/serverbee/bin/serverbee-agent`，install.sh 就会
绕过下载、直接使用它，同时仍然走完 agent.toml + systemd unit 的生成路径。

### 6.1 卸载上一节 docker agent，清 agent.toml，发新 code

```bash
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP "set -e
bash /root/install.sh uninstall agent --yes
rm -f /opt/serverbee/etc/agent.toml /opt/serverbee/docker-compose.agent.yml"

# 上一节 INIT_CODE 已被消费，重新走 recover 拿新 code（也顺带把 has_token 翻 false）
curl -sS -b /tmp/sb.cookies -X POST "https://$DOMAIN/api/servers/$SERVER_ID/recover" \
  -H 'Content-Type: application/json' \
  -d '{"revoke_immediately":true}' | tee /tmp/sb.bincode.json | jq .
export BIN_CODE=$(jq -r '.data.enrollment.code' /tmp/sb.bincode.json)
```

### 6.2 把本地构建的 agent 二进制推上去

```bash
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP "mkdir -p /opt/serverbee/bin"
sshpass -p "$VPS_PASS" scp -o StrictHostKeyChecking=yes \
  docker-bins/linux-amd64/serverbee-agent \
  $VPS_USER@$VPS_IP:/opt/serverbee/bin/serverbee-agent
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP "
  chmod +x /opt/serverbee/bin/serverbee-agent
  /opt/serverbee/bin/serverbee-agent --version 2>&1 | head -1 \
    || /opt/serverbee/bin/serverbee-agent 2>&1 | head -1  # 没 --version 时本地跑会报 missing field server_url，预期"
```

### 6.3 install.sh install agent --method binary

```bash
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP \
  "bash /root/install.sh install agent \
     --method binary \
     --server-url https://${DOMAIN} \
     --enrollment-code ${BIN_CODE} \
     --yes"
```

预期输出包含：

```
[WARN] Binary already exists at /opt/serverbee/bin/serverbee-agent — skipping download (adopting existing)
[INFO] Created /opt/serverbee/etc/agent.toml
Created symlink '/etc/systemd/system/multi-user.target.wants/serverbee-agent.service' → '/etc/systemd/system/serverbee-agent.service'.
[INFO] Agent service started and enabled
```

### 6.4 校验

```bash
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP "
  systemctl is-active serverbee-agent.service
  systemctl status serverbee-agent.service --no-pager | head -10
  grep ExecStart /etc/systemd/system/serverbee-agent.service
  cat /opt/serverbee/etc/agent.toml
"

# Server REST 视角
curl -sS -b /tmp/sb.cookies "https://$DOMAIN/api/servers/$SERVER_ID" \
  | jq '.data | {has_token, agent_version, updated_at}'

# 上线时间线
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP \
  "docker logs serverbee-server 2>&1 | grep -E 'Agent.*(connected|disconnect|unauthorized)' | tail -10"
```

> systemd unit 的 `ExecStart` 是 `/opt/serverbee/bin/serverbee-agent`（无 cap 覆盖时不带参数；
> 你勾选过非默认 capabilities 时会出现 `--caps ...` 参数）。

---

## 7. install.sh uninstall —— 彻底清场

```bash
sshpass -p "$VPS_PASS" ssh $VPS_USER@$VPS_IP <<'REMOTE'
bash /root/install.sh uninstall agent --yes --purge
bash /root/install.sh uninstall server --yes --purge

echo "=== systemd ==="
systemctl list-units --no-legend | grep -i serverbee || echo "no unit"
echo "=== containers ==="
docker ps -a | grep serverbee || echo "no containers"
echo "=== /opt/serverbee ==="
ls /opt/serverbee 2>/dev/null || echo "no /opt/serverbee"
echo "=== management CLI ==="
ls /usr/local/bin/serverbee 2>/dev/null || echo "no CLI"
echo "=== docker volumes ==="
docker volume ls | grep serverbee || echo "no volumes"
echo "=== Caddy + Caddyfile（install.sh 不会动它们） ==="
systemctl is-active caddy
cat /etc/caddy/Caddyfile 2>/dev/null || echo "(Caddyfile cleared by us)"
REMOTE
```

期望：

- systemd unit 移除
- 容器移除
- `/opt/serverbee` 整目录删掉
- `/usr/local/bin/serverbee` CLI 删掉
- docker volume 删掉
- agent **binary** 模式的 `--purge` 不删 docker image（binary 没用 image）；
  agent **docker** 模式的 `--purge` 会删 `ghcr.io/zingerlittlebee/serverbee-agent:*`；
  server 同理。
- Caddy 服务和 Caddyfile **不会** 被 install.sh 动 —— Caddy 不属于 ServerBee 包。
  你想彻底回到裸 VPS 状态需要自己 `systemctl stop caddy && apt-get purge -y caddy && rm /etc/caddy/Caddyfile`。

---

## 8. 实测耗时基线（M3 Max 本机 + Debian 13 VPS）

| 步骤 | 实际耗时 |
| --- | --- |
| 本机 cargo zigbuild cold | ~4 min |
| docker buildx 两镜像（COPY 已编译二进制） | < 1 s |
| docker save / gzip | ~5 s |
| scp 27 MB tarball | ~15 s |
| docker load 两镜像 | ~10 s |
| `install server --domain` 全套（含 Caddy + Let's Encrypt） | ~30 s |
| Onboarding + 创建 server 实体 | ~3 s |
| `install agent --method docker` | ~10 s |
| recover + 卸载 + `install agent --method binary` | ~15 s |
| `uninstall server --purge` + `uninstall agent --purge` | ~10 s |

完整一遍从 cold cache 起约 10 分钟。命中 cache 复跑约 3 分钟（不算交互输入）。

---

## 9. 失败排查表

| 症状 | 排查点 |
| --- | --- |
| install.sh `[ERROR] Failed to get latest version from GitHub` | VPS 出网/DNS 异常；脚本第 745 行 `RESOLVED_VERSION=""` 会清掉外部 env 注入，**不能**靠 env 跳过 |
| Caddy `certificate obtained` 后 HTTPS 仍 502 | install.sh 还没把 server compose `Recreate` 完成；等 5 s 再 curl |
| Caddy 装好但 DNS 检查失败 | A 记录未生效；脚本输出 `[WARN] Skipping DNS check`（带了 `--skip-dns-check`）或 `[ERROR] DNS check failed`，按提示修 A 记录 |
| HTTPS 200 但 `/api/auth/login` 401 | `INIT_PASS` 抄错；从 `docker logs serverbee-server` 重新抓 banner |
| `/api/servers` 返 `MUST_CHANGE_PASSWORD` | 先 `POST /api/auth/onboarding`，**不要** `PUT /api/auth/password`（白名单只放 onboarding） |
| binary 模式 install.sh 仍然去 ghcr 拉镜像 / 仍然 download GitHub | 你忘了 `mkdir -p /opt/serverbee/bin && scp <agent binary> ...`；adopt 路径要求二进制 `chmod +x` 且 `[ -f ... ]` 命中 |
| docker 模式 install.sh 去 ghcr 拉真实 release | 本地镜像 tag 不是 `PROD_TAG`；`docker tag` 改成 release 版本号去 `v` 形态 |
| `systemctl status serverbee-agent` 报 `status=78/CONFIG` | enrollment_code 已过期/已用；按提示 recover 拿新 code、清 `token` 行后 `systemctl restart serverbee-agent` |
| `install.sh install agent` 报 `serverbee-agent is already installed (...). Use 'upgrade'` | meta 残留；先 `uninstall agent --yes`（不带 `--purge` 保留 agent.toml）再 install |
| `uninstall server --purge` 删了我刚才 docker load 的镜像 | 是的，符合预期；从 tarball 重新 `docker load < /root/serverbee-*.tar.gz` 即可 |
| `online` 字段在 REST 永远为 `null` | 设计如此，运行态走 WS push；判定在线看 server 日志 `Agent <id> connected` |

---

## 10. 跟 agent-recover-e2e.md 的关系

- 本文 = 完整正向部署流程，覆盖 server + 两种 agent 模式 + 卸载。
- [agent-recover-e2e.md](agent-recover-e2e.md) = recover 窄回归，专测 install.sh 的 `else` 分支
  （agent.toml 已存在时 `enrollment_code` / `server_url` / `token` 三字段的刷新行为）。
- 改了 install.sh 主路径 / cmd_domain / Caddyfile 生成 / install_server_*：跑本文。
- 改了 recover endpoint / install.sh agent.toml 刷新逻辑 / recover dialog：跑 agent-recover-e2e.md。
- 不确定时跑本文（超集）。
