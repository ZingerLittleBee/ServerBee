# 31 Agent 自动升级 — 冒烟测试

**前置条件**:Agent 启用 CAP_UPGRADE,配置升级来源。深度用例见 [../agent-upgrade.md](../agent-upgrade.md)、[../agent-upgrade-pinned-source.md](../agent-upgrade-pinned-source.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| AU1 | 触发升级 | 服务器详情点击 Upgrade | 下发 Upgrade,Agent 下载新版本 | 是 | ☐ |
| AU2 | 升级进度 | 观察升级过程 | UI 实时显示 UpgradeProgress 阶段 | 是 | ☐ |
| AU3 | 升级完成 | 升级结束后 | Agent 以新版本重连,版本号更新 | 是 | ☐ |
| AU4 | checksum 校验 | 校验 SHA256 | 与 checksums.txt 比对,失败则中止 | 是 | ☐ |
| AU5 | 防降级 | 尝试升级到更低版本 | 拒绝(非升级) | 否 | ☐ |
| AU6 | SPKI pin | pinned-source 配置错误指纹 | TLS 校验失败,升级中止 | 否 | ☐ |
| AU7 | 并发防护 | 升级中再次触发 | 被升级锁拒绝 | 否 | ☐ |
| AU8 | 超时 | 模拟下载超时 | 600s 超时后中止并报错 | 否 | ☐ |

**汇总**:✅ ___ / ❌ ___ / — ___
