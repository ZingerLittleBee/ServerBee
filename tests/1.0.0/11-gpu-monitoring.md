# 11 GPU 监控 — 冒烟测试

**前置条件**:Agent 编译启用 GPU feature 且主机有 NVIDIA GPU(否则本文件整体标 —)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| G1 | GPU 检测 | 启动带 GPU 的 Agent | 上报 GPU 设备数 | 否 | — |
| G2 | 指标采集 | 查看详情页 GPU 区 | 显示显存使用/利用率/温度 | 否 | — |
| G3 | 负载变化 | 跑 GPU 负载 | 利用率/温度随之变化 | 否 | — |
| G4 | 无 GPU 降级 | 无 GPU 主机运行 | 不显示 GPU 区,无报错 | 否 | ✅ |

> G1/G2/G3 —: 测试主机为 macOS Apple M3 Max,无 NVIDIA GPU 且 Agent 未启用 GPU feature,前置条件不满足(整体应标 —)。gpu-records API 返回空,records.gpu_usage 为 null。
> G4 ✅: 无 NVIDIA GPU 主机上 Agent 0.9.3 正常运行未崩溃,详情页正常渲染所有指标且不显示 GPU 区,无报错。

**汇总**:✅ 1 / ❌ 0 / — 3(无 NVIDIA GPU,前置条件不满足)
