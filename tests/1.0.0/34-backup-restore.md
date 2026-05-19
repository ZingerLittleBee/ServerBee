# 34 备份与还原 — 冒烟测试

**前置条件**:已登录 admin,进入 `/settings`(通用设置)。深度用例见 [../general-settings.md](../general-settings.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| BR1 | 下载备份 | 触发备份导出 | 下载到 VACUUM 后的 SQLite 快照 | 是 | ✅ |
| BR2 | 备份完整性 | 检查备份文件 | 可被 sqlite 打开,含主要表数据 | 是 | ✅ |
| BR3 | 还原备份 | 用备份还原到实例 | 还原成功,数据与备份一致 | 是 | ⚠️— |
| BR4 | 通用设置项 | 修改全局设置(如保留期/Key) | 保存并生效 | 否 | ✅ |
| BR5 | 权限 | member 访问备份 | 无权限/入口隐藏 | 否 | ✅ |

> BR1: `POST /api/settings/backup`(VACUUM INTO)HTTP 200,下载 SQLite 文件(503KB),header `SQLite format 3`。
> BR2: sqlite3 可打开备份,主要表齐全(users=2、servers=1,含 sessions/mobile_sessions/server_groups/server_tags 等)。
> BR3: 还原 handler 校验路径已非破坏性验证(非 SQLite / 过小文件均返回 422 拒绝)。**完整 DB-swap 还原跳过**:restore 替换实时 DB 并要求重启 Server(setting.rs:130 "Please restart the server"),重启共享 Server 被约束禁止,且会与其他组并发写竞争 — 跳过以保护测试基线与其他组数据(环境约束,非缺陷)。
> BR4: `GET/PUT /api/settings`(site_name/description/custom_css/custom_js)读写正常;保留期/外观等其他全局设置在 28 等用例已验证持久化。
> BR5: member cookie 访问 `POST /api/settings/backup` 返回 HTTP 403;备份为 admin-only。

**汇总**:✅ 4 / ❌ 0 / — 1(BR3 因共享环境约束跳过完整还原,仅非破坏性校验)
