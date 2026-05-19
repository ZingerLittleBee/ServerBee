# 35 部署与安装(脚本 / Docker / systemd) — 冒烟测试

**前置条件**:干净测试主机(参考可复用测试 VPS)。涉及 `deploy/install.sh`、`Dockerfile`、`docker-compose.yml`。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| DP1 | 脚本安装 Server | `install.sh install`(binary 模式) | 安装到 `/opt/serverbee`,systemd 服务启动 | 是 | ☐ |
| DP2 | 初始管理员 | 首次启动 | 创建管理员账户,可登录 | 是 | ☐ |
| DP3 | Docker Compose | `docker compose up -d` | 容器健康,9527 可访问 | 是 | ☐ |
| DP4 | Agent 一键安装 | 用 enrollment 命令安装 Agent | Agent 注册并上线 | 是 | ☐ |
| DP5 | 服务管理 | `install.sh start/stop/restart/status` | 命令正常控制服务 | 否 | ☐ |
| DP6 | 升级安装 | `install.sh upgrade` | 升级到新版本,数据保留 | 否 | ☐ |
| DP7 | 幂等性 | 重复执行 install | 不破坏现有数据/配置 | 否 | ☐ |
| DP8 | 卸载 | `install.sh uninstall` | 清理服务与文件(按提示保留/删除数据) | 否 | ☐ |
| DP9 | 健康检查 | 访问 `/` 与健康端点 | 返回正常,前端 SPA 加载 | 是 | ☐ |

**汇总**:✅ ___ / ❌ ___ / — ___
