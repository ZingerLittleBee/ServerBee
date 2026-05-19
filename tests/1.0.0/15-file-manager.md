# 15 文件管理 — 冒烟测试

**前置条件**:Agent 启用 CAP_FILE。深度用例见 [../file-manager.md](../file-manager.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| F1 | 目录浏览 | 进入服务器 Files,浏览目录 | 列出文件/子目录,可逐级进入 | 是 | — |
| F2 | 读取/预览 | 打开一个文本文件 | 正确显示内容 | 是 | — |
| F3 | 上传文件 | 上传一个文件到目录 | 上传成功,列表出现 | 是 | — |
| F4 | 下载文件 | 下载(含较大文件) | 文件完整下载,内容一致 | 是 | — |
| F5 | 新建文件夹/重命名 | 创建文件夹并重命名文件 | 操作生效 | 否 | — |
| F6 | 删除 | 删除测试文件 → 确认 | 文件移除 | 否 | — |
| F7 | 权限/黑名单 | 访问受限路径 | 被拒绝,提示无权限 | 否 | — |
| F8 | 能力关闭 | 未启用 CAP_FILE 调用 file list/write API | 被拒 FORBIDDEN server_capability_disabled | 否 | ✅ |

**备注**:测试 agent (0.9.3) agent_local_capabilities=60,本地不支持 CAP_FILE(64);服务端 set FILE 后 effective 仍=60,故 F1–F7 无法真机验证(原因:agent 本地不支持 File 能力,环境限制非缺陷)。F8 已验证:能力关闭时 list/write API 均返回 FORBIDDEN server_capability_disabled,服务端能力拦截正常。

**汇总**:✅ 1 / ❌ 0 / — 7 (—均因测试 agent 本地不支持 FILE 能力)
