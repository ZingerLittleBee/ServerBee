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
| `config set <key> <value>` | 自动检测 | 修改配置项 |
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

## 自动检测逻辑

```bash
detect_installed():
  检查 /usr/local/bin/serverbee-agent   → agent:binary
  检查 /usr/local/bin/serverbee-server  → server:binary
  检查 docker ps -a --filter name=serverbee-agent  → agent:docker
  检查 docker ps -a --filter name=serverbee-server → server:docker
  返回列表: ["agent:binary", "server:docker", ...]
```

检测结果缓存在变量中，一次会话只执行一次。

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
7. Agent：提示手动启动（先检查配置）；Server：直接启动

### 安装流程（docker）

1. 检查 docker + docker compose
2. Agent：Docker 不推荐警告（同当前 install.sh）
3. 生成配置文件 / docker-compose.yml
4. 启动容器

### 安装参数

| 参数 | 适用 | 必须 | 说明 |
|------|------|------|------|
| `--server-url <url>` | agent | 是 | Server HTTP 地址 |
| `--discovery-key <key>` | agent | 是 | 自动注册发现密钥 |
| `--password <pass>` | server | 否 | 管理员密码（默认自动生成） |
| `--method binary\|docker` | 两者 | 否 | 安装方式（默认 binary） |
| `-y` | 两者 | 否 | 跳过确认 |

## uninstall 子命令

```bash
serverbee.sh uninstall agent           # 交互确认
serverbee.sh uninstall agent -y        # 静默
serverbee.sh uninstall agent --purge   # 连配置一起删
```

**组件必须显式指定**，不支持自动检测。

### 卸载流程（binary）

1. 停止 systemd 服务
2. disable + 删除 unit 文件
3. 删除 `/usr/local/bin/serverbee-{component}`
4. `systemctl daemon-reload`
5. `--purge`：删除配置（agent: `/etc/serverbee/agent.toml`，server: `/etc/serverbee/server.toml` + `/var/lib/serverbee`）
6. 无 `--purge`：保留配置文件，打印保留路径

### 卸载流程（docker）

1. `docker stop` + `docker rm` 容器
2. `--purge`：`docker rmi` 镜像 + 删除 compose 文件 + 配置
3. 无 `--purge`：保留镜像和配置

## upgrade 子命令

```bash
serverbee.sh upgrade agent       # 升级 agent
serverbee.sh upgrade server      # 升级 server
serverbee.sh upgrade             # 自动检测，升级所有已安装组件
serverbee.sh upgrade -y          # 静默升级所有
```

### 升级流程（binary）

1. 获取当前版本（`serverbee-{component} --version`）
2. 获取 GitHub latest 版本
3. 版本相同 → 打印 "Already up to date" → 跳过
4. 下载新二进制到 `/tmp`
5. 停止服务
6. 替换二进制
7. 启动服务
8. 打印版本变更摘要

### 升级流程（docker）

1. `docker pull` 最新镜像
2. 重建容器（`docker compose up -d` 或 `docker run`）

## config 子命令

### 查看

```bash
serverbee.sh config              # 显示所有已安装组件的配置
serverbee.sh config agent        # 只看 agent.toml
serverbee.sh config server       # 只看 server.toml
```

输出格式：带语法高亮（如果终端支持）的 TOML 内容，或 plain text fallback。

### 修改

```bash
serverbee.sh config set server_url http://new:9527
serverbee.sh config set collector.interval 5
serverbee.sh config set admin.password newpass
serverbee.sh config set log.level debug
```

#### key 到文件的映射表

| key 前缀 | 目标文件 |
|----------|---------|
| `server_url`, `auto_discovery_key`, `token`, `collector.*`, `file.*`, `ip_change.*` | agent.toml |
| `admin.*`, `server.*`, `auth.*`, `geoip.*`, `retention.*`, `oauth.*` | server.toml |
| `log.*` | 根据已安装组件判断，都装了则两个都改 |

#### 修改逻辑

1. 根据 key 确定目标 TOML 文件
2. 目标文件不存在 → 报错
3. 解析 TOML section（`collector.interval` → `[collector]` section 下的 `interval`）
4. key 已存在 → sed 替换值
5. key 不存在但 section 存在 → 在 section 末尾追加
6. section 不存在 → 追加 section + key
7. 打印变更前后对比
8. 提示是否重启服务生效（`-y` 时自动重启）

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

Docker 模式：从 docker-compose.yml 或 `docker inspect` 读取 environment。

### 设置

```bash
serverbee.sh env set COLLECTOR__INTERVAL 5           # 自动加 SERVERBEE_ 前缀
serverbee.sh env set SERVERBEE_COLLECTOR__INTERVAL 5  # 已有前缀则不重复加
```

#### 写入逻辑

- **binary (systemd)**：写入 `/etc/systemd/system/serverbee-{component}.service.d/override.conf`，然后 `systemctl daemon-reload`
- **docker (compose)**：写入 docker-compose.yml 的 `environment` 段
- **docker (standalone)**：修改后需要重建容器，提示用户确认

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
  Binary:   /usr/local/bin/serverbee-agent v0.7.3
  Config:   /etc/serverbee/agent.toml
  Server:   http://10.0.0.1:9527
  Recent logs (last 5 lines):
    [2026-03-29T10:00:05Z INFO] Connected to server
    ...

Server (docker)
  Container: serverbee-server (Up 3 days)
  Image:     ghcr.io/zingerlittlebee/serverbee-server:0.7.3
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

- 自动检测部署方式（binary → systemctl，docker → docker restart）
- 操作后打印当前状态

## 文件结构变更

| 操作 | 文件 |
|------|------|
| 新增 | `deploy/serverbee.sh` |
| 删除 | `deploy/install.sh` |
| 保留 | `deploy/serverbee-agent.service`（参考模板，脚本内 inline 生成） |
| 保留 | `deploy/serverbee-server.service`（同上） |

## 错误处理

- 所有命令检查 root 权限（需要 sudo），检测失败时给出明确提示
- 网络请求（GitHub API、下载）失败时打印 URL + HTTP 状态码
- 配置文件解析失败时打印具体行号和错误原因
- 幂等：已安装 → "Already installed, use `upgrade` to update"；已卸载 → "Not installed"

## 国际化

沿用当前 install.sh 的双语风格：关键提示中英双语，日志信息英文为主。交互菜单中英并列显示。
