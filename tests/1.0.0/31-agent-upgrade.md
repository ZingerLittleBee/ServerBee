# 31 Agent 自动升级 — 冒烟测试

**前置条件**:Agent 启用 CAP_UPGRADE,配置升级来源。深度用例见 [../agent-upgrade.md](../agent-upgrade.md)、[../agent-upgrade-pinned-source.md](../agent-upgrade-pinned-source.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| AU1 | 触发升级 | POST /api/servers/:id/upgrade | 接口接受请求返回 ok,服务端下发 Upgrade(派发链路验证;实际下载未在共享环境执行) | 是 | — |
| AU2 | 升级进度 | 观察升级过程 | UI 实时显示 UpgradeProgress 阶段 | 是 | — |
| AU3 | 升级完成 | 升级结束后 | Agent 以新版本重连,版本号更新 | 是 | — |
| AU4 | checksum 校验 | 校验 SHA256 | 与 checksums.txt 比对,失败则中止 | 是 | — |
| AU5 | 防降级 | POST upgrade version=0.1.0 (< 当前 0.9.3) | 服务端接受派发,agent 端防降级守卫生效:agent 保持 0.9.3、保持在线、进程未变(pid 58870) | 否 | ✅ |
| AU6 | SPKI pin | pinned-source 配置错误指纹 | TLS 校验失败,升级中止 | 否 | — |
| AU7 | 并发防护 | 升级中再次触发 | 被升级锁拒绝 | 否 | — |
| AU8 | 超时 | 模拟下载超时 | 600s 超时后中止并报错 | 否 | — |

**备注**:UPGRADE(4) 在 agent_local_capabilities=60 中受支持,upgrade 接口可用。AU5 已安全验证防降级:请求 0.1.0 后 agent 仍 0.9.3 在线、进程未重启 — 防降级守卫(agent 端)生效。AU1–AU4/AU6–AU8 需触发真实下载/替换二进制,共享环境无 pinned-source/有效二进制,且 brief 明确警告勿将测试 agent 升级成坏版本掉线影响其他组,故未执行(原因:风险约束 + 环境无升级源,非缺陷)。UI 服务器详情页未见 Upgrade 按钮(疑因未配置升级源)。

**汇总**:✅ 1 / ❌ 0 / — 7 (—均因风险约束/环境无升级源,未执行真实升级)
