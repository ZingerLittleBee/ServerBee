# 16 Web 终端 — 冒烟测试

**前置条件**:Agent 启用 CAP_TERMINAL。深度用例见 [../terminal.md](../terminal.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| TM1 | 打开终端 | 访问 `/terminal/:serverId` | 建立 WS 会话,显示 shell 提示符 | 是 | — |
| TM2 | 执行命令 | 输入 `ls` / `echo hi` | 输出正确回显 | 是 | — |
| TM3 | 交互程序 | 运行 `top` 后退出 | 全屏刷新正常,可正常退出 | 否 | — |
| TM4 | 窗口尺寸 | 调整浏览器窗口 | 终端列宽自适应(resize 生效) | 否 | — |
| TM5 | 关闭会话 | 关闭终端页 | 会话清理,进程回收 | 是 | — |
| TM6 | 会话上限 | 开启超过最大并行会话数 | 超出被拒绝并提示 | 否 | — |
| TM7 | 能力关闭 | 未启用 CAP_TERMINAL 访问终端页 | 优雅降级:显示 "WebSocket connection failed" + Reconnect 按钮,WS 握手被拒 | 否 | ✅ |

**备注**:测试 agent (0.9.3) agent_local_capabilities=60,本地不支持 CAP_TERMINAL(1);服务端 set TERMINAL 后 effective 仍=60(已在 30-capabilities CP5 验证),故 TM1–TM6 无法真机验证(原因:agent 本地不支持 Terminal 能力,环境限制)。TM7 已验证能力关闭时终端 WS 被拒、UI 优雅降级。

**汇总**:✅ 1 / ❌ 0 / — 6 (—均因测试 agent 本地不支持 TERMINAL 能力)
