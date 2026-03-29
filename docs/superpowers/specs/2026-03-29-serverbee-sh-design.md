# serverbee.sh — 交互式管理脚本设计

> 替换 `deploy/install.sh`，提供安装、卸载、升级、配置管理、环境变量管理、服务控制的一站式管理工具。

## 命令总览

```
serverbee.sh                                    # 无参数 → 交互式主菜单
serverbee.sh <command> [component] [options]     # 有参数 → 无人值守
```

| 命令 | 组件要求 | 说明 |
|------|---------|------|
| `install [agent\|server]` | 可选（交互选择） | 安装组件 |
| `uninstall <agent\|server>` | **必须** | 卸载组件 |
| `upgrade [agent\|server]` | 可选（自动检测） | 升级到最新版 |
| `status` | 自动检测 | 运行状态 + 最近日志 |
| `start` / `stop` / `restart` | 自动检测 | 服务控制 |
| `config` | 自动检测 | 查看当前配置 |
| `config set <key> <value>` | 自动检测 | 修改配置项（仅标量类型） |
| `env` | 自动检测 | 列出 SERVERBEE_* 环境变量 |
| `env set <key> <value>` | 自动检测 | 写入 systemd override |

全局 flag：
- `-y` — 跳过所有交互确认
- `--method binary|docker` — 指定安装方式（默认 binary）

## 设计原则

- **无参数 = 交互，有参数 = 静默**：同一套逻辑，交互模式只是参数收集方式不同
- **查询自动检测，变更显式指定**：status/config/env/start/stop/restart 自动检测已安装组件；uninstall/upgrade 要求指定组件（upgrade 无参数时检测所有已装的，逐个确认）
- **单文件**：保持 `curl -fsSL ... | bash` 友好
- **幂等**：重复执行不出错（已安装则跳过，已卸载则提示）
- **只管理自身安装的实例**：安装时写入元数据文件，后续操作依据元数据而非现场推断

## 安装元数据

安装时在 `/etc/serverbee/.install-meta` 写入 JSON 元数据，所有后续命令依据此文件判断状态：

```json
{
  "agent": {
    "method": "binary",
    "version": "v0.7.3",
    "installed_at": "2026-03-29T10:00:00Z"
  },
  "server": {
    "method": "docker",
    "version": "v0.7.3",
    "installed_at": "2026-03-29T10:00:00Z"
  }
}
```

- `install` 写入/更新对应组件条目
- `uninstall` 移除对应组件条目
- `upgrade` 更新 version 字段
- 元数据文件不存在或为空 → 视为未安装任何组件
- **不兼容旧版安装**：此脚本只管理由自身安装的实例。无元数据的安装（手动安装或旧版 install.sh 安装的）不会被 fallback 接管。见下文"旧版安装处理策略"。

## 自动检测逻辑

**严格依赖安装元数据**，不做 heuristic 探测：

```bash
detect_installed():
  读取 /etc/serverbee/.install-meta
  文件存在且有对应组件条目 → 返回 [(component, method, version)]
  文件不存在或无条目 → 返回空列表
```

检测结果缓存在变量中，一次会话只执行一次。

### 旧版安装处理策略

对于没有 `.install-meta` 但实际存在二进制/容器的旧版安装：

- **查询类命令**（status）：额外检测 binary/container 存在性，如果发现未纳管的实例，打印提示：
  ```
  [WARN] Found serverbee-agent at /usr/local/bin/serverbee-agent but it is not managed by this script.
         To bring it under management, run: serverbee.sh install agent [options]
         (This will write metadata only — existing binary and config will be preserved.)
  ```
- **变更类命令**（install/uninstall/upgrade/config set/env set/start/stop/restart）：**只操作有元数据的实例**。对未纳管的实例不做任何操作，不尝试猜测其部署方式。
- **install 纳管（binary）**：对已存在的 binary 或配置执行 `install` 时，检测到 binary 已存在则跳过下载，检测到 config 已存在则跳过生成，只写入元数据和 systemd unit（如需要）。这提供了一条安全的"纳管"路径。
- **install 纳管（docker）**：**不支持自动纳管**。如果检测到同名未纳管容器（`docker ps -a --filter name=serverbee-{component}` 存在但 `.install-meta` 无对应条目），`install --method docker` 拒绝执行并报错：
  ```
  [ERROR] Found existing container 'serverbee-agent' not managed by this script.
          Please remove it first:  docker stop serverbee-agent && docker rm serverbee-agent
          Then re-run:  serverbee.sh install agent --method docker ...
  ```
  这避免了新 compose 与旧 standalone 容器的端口/命名冲突。

## 交互式主菜单

无参数运行时：

```
ServerBee Manager
=================

  [1] Install    安装
  [2] Uninstall  卸载
  [3] Upgrade    升级
  [4] Status     查看状态
  [5] Service    服务控制 (start/stop/restart)
  [6] Config     配置管理
  [7] Env        环境变量
  [0] Exit       退出

Select [0-7]:
```

选择后进入对应子菜单，交互引导完成操作。子菜单内部复用子命令的同一套函数。

## install 子命令

### 非交互模式

```bash
# Agent（binary，默认）
serverbee.sh install agent --server-url http://x:9527 --discovery-key abc -y

# Agent（docker）
serverbee.sh install agent --method docker --server-url http://x:9527 --discovery-key abc -y

# Server（binary，密码自动生成）
serverbee.sh install server -y

# Server（docker，指定密码）
serverbee.sh install server --method docker --password mypass -y
```

### 交互模式

`serverbee.sh install` → 选组件 → 选方式 → 填参数（复用当前 install.sh 的 prompt 流程）。

### 安装流程（binary）

1. 检测 OS + arch
2. 获取 GitHub latest release 版本
3. 下载 `serverbee-{component}-{os}-{arch}` → `/usr/local/bin/serverbee-{component}`
4. 生成配置文件到 `/etc/serverbee/{component}.toml`（已存在则跳过）
5. 创建 systemd unit 文件（inline 生成，不依赖外部模板）
6. `systemctl daemon-reload && enable`
7. 写入安装元数据（version、method、时间戳）
8. Agent：提示手动启动（先检查配置）；Server：直接启动

### 安装流程（docker）

Docker 统一使用 docker compose 模型，不再使用 `docker run`。

1. 检查 docker + docker compose
2. **冲突检测**：检查是否存在同名未纳管容器 → 存在则报错并指引清理（见旧版安装处理策略）
3. Agent：Docker 不推荐警告（同当前 install.sh）
4. 获取 GitHub latest release tag（作为版本号来源）
5. 生成 `/etc/serverbee/{component}.toml` 配置文件
6. 生成 `/opt/serverbee/docker-compose.{component}.yml`（镜像 tag 使用 release tag，如 `:v0.7.3`，而非 `:latest`）
7. `docker compose -f /opt/serverbee/docker-compose.{component}.yml up -d`
8. 写入安装元数据（version = release tag）

server 和 agent 各自独立 compose 文件，互不干扰，支持单独升级/卸载。

### 安装参数

| 参数 | 适用 | 必须 | 说明 |
|------|------|------|------|
| `--server-url <url>` | agent | 是 | Server HTTP 地址 |
| `--discovery-key <key>` | agent | 是 | 自动注册发现密钥 |
| `--password <pass>` | server | 否 | 管理员初始密码（仅首次启动生效，默认自动生成） |
| `--method binary\|docker` | 两者 | 否 | 安装方式（默认 binary） |
| `-y` | 两者 | 否 | 跳过确认 |

## uninstall 子命令

```bash
serverbee.sh uninstall agent           # 交互确认
serverbee.sh uninstall agent -y        # 静默
serverbee.sh uninstall agent --purge   # 连配置和数据一起删
```

**组件必须显式指定**，不支持自动检测。

### 卸载流程（binary）

1. 停止 systemd 服务
2. disable + 删除 unit 文件 + 删除 override.conf（如有）
3. 删除 `/usr/local/bin/serverbee-{component}`
4. `systemctl daemon-reload`
5. 从安装元数据中移除该组件
6. `--purge`：删除配置（agent: `/etc/serverbee/agent.toml`，server: `/etc/serverbee/server.toml` + `/var/lib/serverbee`）
7. 无 `--purge`：保留配置文件，打印保留路径

### 卸载流程（docker）

1. `docker compose -f /opt/serverbee/docker-compose.{component}.yml down`
2. 从安装元数据中移除该组件
3. `--purge`：`docker rmi` 镜像 + `docker volume rm` 相关命名卷 + 删除 compose 文件 + 配置文件
4. 无 `--purge`：保留镜像、卷和配置

## upgrade 子命令

```bash
serverbee.sh upgrade agent       # 升级 agent
serverbee.sh upgrade server      # 升级 server
serverbee.sh upgrade             # 自动检测，升级所有已安装组件
serverbee.sh upgrade -y          # 静默升级所有
```

### 版本获取策略

由于二进制当前不支持 `--version` 参数，版本信息通过以下方式获取：

1. **首选**：读取安装元数据中的 `version` 字段
2. **Fallback**：跳过版本比较，直接下载最新版替换（打印 "Cannot determine current version, downloading latest..."）

### 升级流程（binary）

1. 读取安装元数据获取当前版本
2. 获取 GitHub latest 版本
3. 版本相同 → 打印 "Already up to date (vX.Y.Z)" → 跳过
4. 下载新二进制到 `/tmp`
5. 停止服务
6. 替换二进制
7. 启动服务
8. 更新安装元数据中的 version
9. 打印版本变更摘要

### 升级流程（docker）

1. 获取 GitHub latest release tag
2. 比较元数据中的 version 与 latest tag → 相同则跳过
3. 更新 compose 文件中的镜像 tag（如 `:v0.7.3` → `:v0.8.0`）
4. `docker compose -f /opt/serverbee/docker-compose.{component}.yml pull`
5. `docker compose -f /opt/serverbee/docker-compose.{component}.yml up -d`
6. 更新安装元数据中的 version

### Docker 版本策略

Docker 镜像统一使用**精确版本 tag**（如 `ghcr.io/zingerlittlebee/serverbee-server:v0.7.3`），不使用 `:latest`。版本号来源统一为 GitHub latest release tag（与 binary 安装共用同一个 `get_latest_version()` 函数）。这确保：
- 安装元数据中的 version 与实际运行镜像一致
- upgrade 能可靠比较版本
- 不会因 Docker daemon 缓存的 `:latest` 导致版本混乱

## config 子命令

### 查看

```bash
serverbee.sh config              # 显示所有已安装组件的配置
serverbee.sh config agent        # 只看 agent.toml
serverbee.sh config server       # 只看 server.toml
```

输出格式：带语法高亮（如果终端支持）的 TOML 内容，或 plain text fallback。

### 修改

`config set` **仅支持标量类型**（string / number / bool）。数组类型的 key（如 `file.root_paths`、`server.trusted_proxies`、`oauth.oidc.scopes`、`file.deny_patterns`）不支持通过 `config set` 修改，尝试修改时提示用户直接编辑 TOML 文件。

```bash
serverbee.sh config set server_url http://new:9527
serverbee.sh config set collector.interval 5
serverbee.sh config set log.level debug
```

#### key 到文件的映射表

| key 前缀 | 目标文件 |
|----------|---------|
| `server_url`, `auto_discovery_key`, `token` | agent.toml |
| `collector.*`, `ip_change.*` | agent.toml |
| `file.enabled`, `file.max_file_size` (agent 端标量 key) | agent.toml |
| `file.max_upload_size` (server 端) | server.toml |
| `server.*`, `auth.*`, `geoip.*`, `retention.*`, `oauth.*` | server.toml |
| `database.*`, `rate_limit.*`, `scheduler.*`, `upgrade.*` | server.toml |
| `log.*` | 根据已安装组件判断，都装了则两个都改 |

不在映射表中的 key → 报错 "Unknown config key: xxx"。

#### 不可修改的 key

以下 key 仅在 server 首次启动（用户表为空）时生效，运行时修改无实际效果，因此 `config set` 拒绝修改并给出引导：

- `admin.password` → 提示 "Admin password can only be set during initial installation. To change password, use the Dashboard UI."
- `admin.username` → 提示 "Admin username can only be set during initial installation."

#### 修改逻辑

1. **先检查拒绝列表**：不可修改 key（`admin.password`、`admin.username`）→ 报错并给出定向引导
2. **再检查数组类型**：`file.root_paths`、`file.deny_patterns`、`server.trusted_proxies`、`oauth.oidc.scopes` → 报错提示手动编辑 TOML 文件
3. 根据 key 查映射表确定目标 TOML 文件（不在映射表中 → "Unknown config key"）
3. 目标文件不存在 → 报错
4. 解析 TOML section（`collector.interval` → `[collector]` section 下的 `interval`）
5. key 已存在 → sed 替换值（仅替换 `key = value` 行，精确匹配 key 名）
6. key 不存在但 section 存在 → 在 section 末尾追加
7. section 不存在 → 追加 section + key
8. 打印变更前后对比
9. 提示是否重启服务生效（`-y` 时自动重启）

## env 子命令

### 查看

```bash
serverbee.sh env                 # 列出所有相关环境变量
```

输出三个来源，标注优先级：

```
Environment Variables
=====================

Source: shell
  (none)

Source: systemd override (serverbee-agent)
  SERVERBEE_COLLECTOR__INTERVAL=5

Source: systemd override (serverbee-server)
  SERVERBEE_SERVER__DATA_DIR=/var/lib/serverbee

Note: env vars override TOML config values
```

Docker 模式：从 docker-compose.yml 读取 environment 段。

### 设置

```bash
serverbee.sh env set COLLECTOR__INTERVAL 5           # 自动加 SERVERBEE_ 前缀
serverbee.sh env set SERVERBEE_COLLECTOR__INTERVAL 5  # 已有前缀则不重复加
```

#### 写入逻辑

- **binary (systemd)**：写入 `/etc/systemd/system/serverbee-{component}.service.d/override.conf`，然后 `systemctl daemon-reload`
- **docker (compose)**：修改 `/opt/serverbee/docker-compose.{component}.yml` 的 `environment` 段，然后 `docker compose up -d` 重建

key 到组件的映射：同 config set 的映射表。`COLLECTOR__*` → agent，`SERVER__*` → server，`ADMIN__*` → server。

## status 子命令

```bash
serverbee.sh status
```

自动检测所有已安装组件，输出示例：

```
ServerBee Status
================

Agent (binary)
  Service:  active (running) since 2026-03-29 10:00:00
  Version:  v0.7.3 (from install metadata)
  Binary:   /usr/local/bin/serverbee-agent
  Config:   /etc/serverbee/agent.toml
  Server:   http://10.0.0.1:9527
  Recent logs (last 5 lines):
    [2026-03-29T10:00:05Z INFO] Connected to server
    ...

Server (docker)
  Container: serverbee-server (Up 3 days)
  Version:   v0.7.3 (from install metadata)
  Image:     ghcr.io/zingerlittlebee/serverbee-server:v0.7.3
  Port:      0.0.0.0:9527->9527
  Dashboard: http://10.0.0.1:9527
  Recent logs (last 5 lines):
    ...

No issues detected.
```

无已安装组件时：提示 "No ServerBee components found. Run `serverbee.sh install` to get started."

## start / stop / restart 子命令

```bash
serverbee.sh start               # 启动所有已安装组件
serverbee.sh stop agent          # 停止指定组件
serverbee.sh restart server      # 重启指定组件
```

- 根据安装元数据确定部署方式（binary → systemctl，docker → docker compose）
- 操作后打印当前状态

## 文件结构变更

| 操作 | 文件 |
|------|------|
| 新增 | `deploy/serverbee.sh`（主脚本） |
| 修改 | `deploy/install.sh` → 与 serverbee.sh 相同内容（stdin 兼容，见下文） |
| 更新 | `README.md`, `README.zh-CN.md` — 更新安装命令示例 |
| 更新 | `apps/docs/content/docs/{en,cn}/agent.mdx` — 更新 curl 安装链接 |
| 更新 | `apps/docs/content/docs/{en,cn}/quick-start.mdx` — 更新 curl 安装链接 |
| 保留 | `deploy/serverbee-agent.service`（参考模板，脚本内 inline 生成） |
| 保留 | `deploy/serverbee-server.service`（同上） |

### install.sh 向后兼容

**`deploy/install.sh` 是 `deploy/serverbee.sh` 的完整副本**，两个文件代码完全相同。

主脚本（无论叫什么文件名）统一支持一个 shorthand：**第一个参数不是已知子命令时，自动注入 `install`**。这是主脚本的正式功能，不区分文件名：

```bash
# Known subcommands
KNOWN_COMMANDS="install uninstall upgrade status start stop restart config env"

# Shorthand: if first arg is not a known command, prepend "install"
# Examples:
#   serverbee.sh server                          → install server
#   serverbee.sh agent --server-url ...          → install agent --server-url ...
#   curl ... | bash -s server                    → install server
#   bash deploy/install.sh agent                 → install agent
if [[ $# -gt 0 ]] && ! echo "$KNOWN_COMMANDS" | grep -qw "$1"; then
    set -- install "$@"
fi
```

这意味着以下形式全部等价：
- `serverbee.sh install server` = `serverbee.sh server`
- `serverbee.sh install agent --server-url ...` = `serverbee.sh agent --server-url ...`
- `curl ... deploy/install.sh | sudo bash -s server` → `install server`
- `sudo bash deploy/install.sh agent` → `install agent`

命令总览中 `serverbee.sh <command> [component] [options]` 仍然是标准形式；shorthand 是便捷语法，不影响子命令的完整性。无参数时仍然进入交互式菜单。

发布时 CI 将 `deploy/serverbee.sh` 复制为 `deploy/install.sh`（或两个文件始终保持相同内容）。

## 运行时依赖

| 路径 | 用途 | 创建时机 |
|------|------|---------|
| `/etc/serverbee/` | 配置文件目录 | install |
| `/etc/serverbee/.install-meta` | 安装元数据（JSON） | install |
| `/etc/serverbee/agent.toml` | Agent 配置 | install agent |
| `/etc/serverbee/server.toml` | Server 配置 | install server |
| `/var/lib/serverbee/` | Server 数据目录 | install server (binary) |
| `/opt/serverbee/` | Docker compose 文件目录 | install (docker) |
| `/opt/serverbee/docker-compose.agent.yml` | Agent compose 定义 | install agent --method docker |
| `/opt/serverbee/docker-compose.server.yml` | Server compose 定义 | install server --method docker |

## 错误处理

- 所有命令检查 root 权限（需要 sudo），检测失败时给出明确提示
- 网络请求（GitHub API、下载）失败时打印 URL + HTTP 状态码
- 配置文件解析失败时打印具体行号和错误原因
- 幂等：已安装 → "Already installed, use `upgrade` to update"；已卸载 → "Not installed"

## 国际化

沿用当前 install.sh 的双语风格：关键提示中英双语，日志信息英文为主。交互菜单中英并列显示。
