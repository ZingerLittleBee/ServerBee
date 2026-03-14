# ServerBee 测试指南

## 快速命令

```bash
# 全量测试
cargo test --workspace && bun run test

# Rust 测试（52+ 单元 + 2 集成）
cargo test --workspace

# 前端测试（14 vitest）
bun run test

# 代码质量
cargo clippy --workspace -- -D warnings
bun x ultracite check
bun run typecheck
```

## Rust 测试

### 按 crate 运行

```bash
cargo test -p serverbee-common          # 协议 + 能力常量 (15 tests)
cargo test -p serverbee-server          # 服务端单元 + 集成 (39 tests)
```

### 仅集成测试

```bash
cargo test -p serverbee-server --test integration
```

集成测试会启动真实 server + SQLite 临时数据库，无需外部依赖。

### 运行单个测试

```bash
cargo test -p serverbee-server test_hash_and_verify_password
cargo test --workspace -- --nocapture   # 显示 stdout
```

### 单元测试覆盖

| 模块 | 测试数 | 覆盖内容 |
|------|--------|----------|
| `common/constants.rs` | 10 | 能力位运算、默认值、掩码 |
| `common/protocol.rs` | 5 | 消息序列化/反序列化 |
| `server/service/alert.rs` | 15 | 阈值判定、指标提取、采样窗口 |
| `server/service/auth.rs` | 7 | 密码哈希、session token、API key、TOTP |
| `server/service/notification.rs` | 11 | 模板变量替换、渠道配置解析 |
| `server/service/record.rs` | 4 | 历史查询、聚合、清理策略 |

### 集成测试覆盖

| 测试 | 流程 |
|------|------|
| `test_agent_register_connect_report` | Agent 注册 → WS 连接 → SystemInfo → 指标上报 |
| `test_backup_restore` | 创建数据 → 备份 → 恢复 → 验证完整性 |

## 前端测试

```bash
bun run test              # 单次运行（CI 用）
bun run test:watch        # 监听模式（开发用）

# 单个文件
cd apps/web && bunx vitest run src/lib/capabilities.test.ts
```

### 前端测试覆盖

| 文件 | 测试数 | 覆盖内容 |
|------|--------|----------|
| `capabilities.test.ts` | 6 | hasCap、toggle on/off、默认值 |
| `use-auth.test.tsx` | 4 | 登录/登出状态、fetch mock |
| `use-api.test.tsx` | 4 | server/records 数据获取、空 id 守卫 |

### 测试工具

- **vitest** — 测试框架（jsdom 环境）
- **@testing-library/react** — React 组件测试
- **@testing-library/jest-dom** — DOM 断言匹配器

## 代码质量检查

```bash
# Rust: clippy 0 warnings（CI 强制）
cargo clippy --workspace -- -D warnings

# 前端: Biome lint + format
bun x ultracite check      # 检查
bun x ultracite fix         # 自动修复

# TypeScript 类型检查（含 fumadocs）
bun run typecheck
```

## 手动功能验证

### 启动本地环境

```bash
# 方式一：源码
cargo run -p serverbee-server &
cargo run -p serverbee-agent &

# 方式二：Docker
docker compose up -d
```

默认地址：`http://localhost:9527`，管理员：`admin`

### 验证清单

| 功能 | 验证方法 |
|------|----------|
| 登录 | `http://localhost:9527` 用 admin 登录 |
| Agent 连接 | Dashboard 显示服务器上线 |
| 实时指标 | CPU/内存/磁盘图表实时更新 |
| Swagger UI | `http://localhost:9527/swagger-ui/` 加载正常 |
| 告警 | 创建 CPU > 80% 规则，验证触发 |
| 终端 | 服务器详情 → Terminal 按钮 → 执行命令 |
| Ping | 创建 HTTP 探测任务 → 查看结果图表 |
| 状态页 | `http://localhost:9527/status` 无需登录可见 |
| 2FA | Settings → Security → Setup 2FA |
| 功能开关 | Settings → Capabilities → toggle Terminal |
| 备份恢复 | Settings → Backup → Download → Restore |

## 测试文件位置

```
crates/common/src/constants.rs          # 能力常量测试
crates/common/src/protocol.rs           # 协议序列化测试
crates/server/src/service/alert.rs      # 告警服务测试
crates/server/src/service/auth.rs       # 认证服务测试
crates/server/src/service/notification.rs # 通知服务测试
crates/server/src/service/record.rs     # 记录服务测试
crates/server/tests/integration.rs      # 集成测试
apps/web/src/lib/capabilities.test.ts   # 能力位测试
apps/web/src/hooks/use-auth.test.tsx    # Auth hook 测试
apps/web/src/hooks/use-api.test.tsx     # API hook 测试
apps/web/vitest.config.ts               # Vitest 配置
.github/workflows/ci.yml               # CI 流水线
```
