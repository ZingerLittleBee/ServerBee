# Dashboard 可拖拽小组件重构设计

## 1. 背景

当前 dashboard 小组件编辑能力已可用，但拖拽过程中的交互正确性不稳定。现状中，页面层、Grid 渲染层、编辑草稿层的职责交织在一起：

- `apps/web/src/routes/_authed/index.tsx` 同时负责数据获取、编辑态、草稿管理、保存组装。
- `apps/web/src/components/dashboard/dashboard-grid.tsx` 同时负责布局转换、桌面端拖拽、移动端分支、编辑浮层和事件回传。
- 拖拽过程通过 `onLayoutChange` 持续把整份布局回写到父层，父层再重建 `widgets -> layout`，形成受控闭环。

这会导致以下典型问题：

- 拖拽过程中组件跳位、抖动、重排不可预测。
- Grid 内部正在交互时，被外部 state 重喂 layout 打断。
- 新增、删除、编辑配置、切换 dashboard 等业务操作与布局交互互相污染。

本次重构目标是在保留 `react-grid-layout` 的前提下，优先解决拖拽过程中的状态错乱和布局不稳定问题。

## 2. 目标与非目标

### 2.1 目标

- 保留 `react-grid-layout`，不更换拖拽库。
- 消除拖拽/缩放过程中的跳位、抖动和不可预测重排。
- 明确页面层、编辑草稿层、Grid 交互层的职责边界。
- 将“布局编辑”和“内容编辑”拆成独立的数据通道。
- 为后续断点布局、撤销/重做、局部保存保留可扩展边界，但本次不实现这些能力。

### 2.2 非目标

- 不修改服务端 dashboard/widget 数据模型。
- 不引入新的布局系统或自定义拖拽引擎。
- 不在本次重构中实现多断点布局。
- 不在本次重构中实现撤销/重做、拖拽占位预览、协同编辑。

## 3. 根因分析

当前抖动问题的核心不是单一参数设置，而是状态流错误：

1. Grid 在拖拽过程中通过 `onLayoutChange` 持续向父层回写完整布局。
2. 父层收到回写后更新 `draftWidgets`。
3. `DashboardGrid` 再根据新的 `widgets` 重新计算 `layout`。
4. `react-grid-layout` 正在处理交互时又收到新的受控布局输入，导致内部计算被打断。

因此，本次重构必须优先修正状态边界和事件提交时机，而不是继续围绕参数做补丁式修复。

## 4. 方案选择

设计阶段评估了三类方案：

### 4.1 方案 A：最小修补

- 保持现有结构。
- 仅把提交时机从 `onLayoutChange` 改成 `onDragStop` / `onResizeStop`。
- 增加更多 diff 判断和同步保护。

优点：

- 改动小，落地快。

缺点：

- 页面层、草稿层、Grid 仍旧耦合。
- 只能缓解当前问题，后续新增/删除/断点布局仍容易再次失稳。

### 4.2 方案 B：编辑器状态内核 + 半受控 Grid

- 引入专门的 dashboard editor 草稿层。
- Grid 内部维护拖拽中的瞬时布局。
- 拖拽结束后才向 editor 提交最终布局 patch。

优点：

- 可以直接切断 drag-time 抖动根因。
- 保留 `react-grid-layout`，重构规模可控。
- 后续扩展空间足够。

缺点：

- 需要中等规模重构，不是纯参数修补。

### 4.3 方案 C：布局适配层 + 命令模型

- 在方案 B 基础上进一步抽象 command 系统和 layout adapter。
- 为撤销/重做、多断点布局提前做完整建模。

优点：

- 长期扩展性最佳。

缺点：

- 对本次“先修拖拽正确性”目标偏重。

### 4.4 最终选择

采用方案 B。

原因：

- 它直接处理当前问题的根因。
- 它保留现有技术栈和 widget 数据模型。
- 它能在控制改动范围的同时，建立清晰的可扩展边界。

## 5. 设计概览

重构后的状态边界分为三层：

- 页面层：负责 dashboard 数据拉取、切换、编辑模式控制、保存和取消。
- 编辑器层：负责 widget 草稿的唯一业务真相。
- Grid 层：负责拖拽中的瞬时布局和交互状态。

核心原则：

- 拖拽过程中只更新 Grid 内部的 `liveLayout`。
- 只有在 `drag stop` / `resize stop` 时，才提交布局 patch 给 editor。
- editor 只维护业务草稿，不参与每一帧拖拽回写。
- 页面层不再直接拼装拖拽期状态，只消费 editor 的稳定结果。

## 6. 模块拆分

### 6.1 `use-dashboard-editor.ts`

新增专门的编辑器 hook，负责 dashboard 编辑草稿管理。

建议能力：

- `startEditing(widgets)`
- `cancelEditing()`
- `commitLayoutPatch(patch)`
- `addWidget(widgetDraft)`
- `updateWidget(id, changes)`
- `deleteWidget(id)`
- `buildSaveInput()`
- `isDirty`

职责要求：

- 它是唯一的业务草稿源。
- 它负责 layout patch 合并，但不负责拖拽中的瞬时交互态。
- 它对外暴露稳定的编辑命令，而不是让页面层直接散落状态更新逻辑。

### 6.2 `dashboard-layout.ts`

新增纯函数模块，负责布局转换和 patch 合并。

建议函数：

- `widgetsToLayout(widgets)`
- `layoutToPatch(layout, prevWidgets)`
- `mergeLayoutPatch(widgets, patch)`
- `normalizeNewWidgetPlacement(widgets, newWidget)`

职责要求：

- 不依赖 React。
- 聚合 widget 与 grid layout 的相互转换逻辑。
- 保证 patch 只影响有变化的 widget。

### 6.3 `dashboard-grid.tsx`

保留现有组件，但显著收敛职责：

- 维护内部 `liveLayout`
- 管理 `interactionState`
- 监听 `onDragStart/onDragStop/onResizeStart/onResizeStop`
- 对外只暴露 `onLayoutCommit`
- 根据屏幕尺寸保留桌面 grid / 移动端 list 双分支

Grid 不再负责：

- 业务草稿存储
- 保存 payload 组装
- 拖拽过程中的父层全量回写

### 6.4 `routes/_authed/index.tsx`

页面层收敛为 orchestration：

- 拉取 dashboard 与 widgets
- 切换 dashboard
- 控制进入编辑、取消编辑、保存
- 管理 picker / config dialog 开关
- 将新增、删除、编辑配置、布局提交等命令转发给 editor

## 7. 状态模型

### 7.1 业务草稿

编辑器层维护 canonical draft widgets：

- `draftWidgets`
- `isEditing`
- `isDirty`

`draftWidgets` 是保存时的唯一数据来源。

### 7.2 Grid 交互态

Grid 内部维护：

- `liveLayout`
- `interactionState = 'idle' | 'dragging' | 'resizing'`

`liveLayout` 仅用于承载 `react-grid-layout` 的交互结果，不直接代表最终业务真相。

### 7.3 外部同步规则

Grid 只接受两类外部 layout 同步：

- 非编辑态下，服务端 widgets 变化时，整体同步到 Grid。
- 编辑态但 `interactionState === 'idle'` 时，editor draft 变化可同步到 Grid。

当 `interactionState !== 'idle'` 时：

- 禁止外部 layout 覆盖 `liveLayout`。
- 直到交互结束后，才允许重新对齐外部数据。

这条规则是本次重构避免 drag-time 抖动的关键。

## 8. 事件流设计

### 8.1 进入编辑

页面层基于当前 dashboard widgets 初始化 editor draft。

### 8.2 Grid 初始化

`DashboardGrid` 从 editor draft 派生初始 `liveLayout`。

### 8.3 拖拽/缩放进行中

- 仅更新 Grid 内部 `liveLayout`
- 不回写 `draftWidgets`
- 不触发页面层整树重算

### 8.4 拖拽/缩放结束

在 `onDragStop` / `onResizeStop` 中：

- 通过 `layoutToPatch` 计算最终 patch
- 若 patch 为空，则不触发 state 更新
- 若 patch 非空，调用 `commitLayoutPatch`

### 8.5 editor 合并 patch

editor 仅更新对应 widget 的：

- `grid_x`
- `grid_y`
- `grid_w`
- `grid_h`

不触碰：

- `title`
- `config_json`
- `widget_type`
- 非受影响 widget 的业务字段

### 8.6 保存

保存时仅由 editor 的 canonical draft 生成 API payload。

页面层不再通过当前 Grid 状态临时拼 payload。

## 9. 新增、删除、编辑配置规则

### 9.1 新增 widget

- editor 生成新 widget 草稿
- 默认带上 widget type 对应尺寸
- 初始位置使用安全策略，默认 `x = 0, y = Infinity`
- 再由 `normalizeNewWidgetPlacement` 做一次位置归一化

### 9.2 删除 widget

- editor 删除对应草稿项
- Grid 在下一个 `idle` 同步周期重建 `liveLayout`

### 9.3 编辑配置

- 仅更新 `title` 和 `config_json`
- 不修改 layout 字段

目标是确保“内容编辑”和“布局编辑”走两条独立通道，避免彼此污染。

## 10. `sort_order` 规则

当前实现会在保存时通过数组索引重建 `sort_order`，容易让布局语义和顺序语义纠缠在一起。

本次重构明确约束：

- 拖拽只修改 `grid_x/y/w/h`
- `sort_order` 只在新增、删除、显式列表重排时变化
- 单次拖拽不会顺手改写整个数组的顺序语义

保存时：

- editor 按稳定草稿顺序输出 payload
- 不因一次 drag 引发整表 `sort_order` 非必要变化

## 11. dashboard 切换与编辑态切换

以下时机必须执行硬重置，而不是试图复用旧交互态：

- 进入编辑：从当前 dashboard 重新创建 draft
- 取消编辑：丢弃 draft，恢复服务端 widgets
- 切换 dashboard：退出编辑并销毁旧 editor 状态
- 保存成功：以服务端返回结果整体替换本地展示态和编辑源

原因：

- 拖拽编辑器最怕半旧半新的混合状态。
- 对这些切换点采用保守重建策略，比智能复用更可靠。

## 12. 移动端策略

移动端继续降级为非拖拽列表，但边界需要更明确：

- 移动端不维护桌面 Grid 的瞬时布局
- 切换到移动端时立即结束交互态
- 切回桌面端时，从 editor draft 重新生成 `liveLayout`

目标是避免桌面端 Grid 内部状态跨 breakpoint 污染移动端或返程桌面端行为。

## 13. 错误处理与保存策略

- 若 `onLayoutCommit` 计算出的 patch 为空，则不触发任何更新。
- 保存失败时保留 editor draft，允许用户继续调整或重试。
- 保存成功后优先使用服务端返回 widgets 重建本地状态，避免继续持有旧布局。
- 新 widget 位置归一化失败时，回退到 `x = 0, y = Infinity` 的安全策略，由 grid 自动安置。

## 14. 测试策略

### 14.1 `dashboard-layout.ts` 纯函数测试

覆盖点：

- layout 变化仅生成受影响 widget 的 patch
- 空 patch 不修改数据
- `mergeLayoutPatch` 只改 layout 字段
- 新增/删除后的归一化位置符合预期

### 14.2 `DashboardGrid` 组件测试

覆盖点：

- 拖拽进行中不会调用父层 layout commit
- `drag stop` / `resize stop` 才触发 commit
- 外部 widgets 更新在 `idle` 时同步，在 `dragging` / `resizing` 时不覆盖 `liveLayout`
- 移动端不渲染 grid，且不会残留桌面交互态

### 14.3 `useDashboardEditor` 测试

覆盖点：

- `commitLayoutPatch` 只改网格字段
- `updateWidget` 不污染 layout
- `deleteWidget` / `addWidget` 后草稿正确
- `buildSaveInput` 输出稳定，不因为单次 drag 改坏 `sort_order`

### 14.4 页面级流程测试

覆盖点：

- 进入编辑 -> 拖拽 -> 保存，payload 正确
- 进入编辑 -> 拖拽 -> 取消，恢复服务端状态
- 新增 widget 后拖拽，再保存，payload 正确
- 切换 dashboard 时正确销毁旧编辑态

## 15. 实施范围

本次实现预计主要涉及：

- `apps/web/src/routes/_authed/index.tsx`
- `apps/web/src/components/dashboard/dashboard-grid.tsx`
- `apps/web/src/hooks/use-dashboard-editor.ts`
- `apps/web/src/components/dashboard/dashboard-layout.ts`
- 对应测试文件

本次不涉及：

- Rust 服务端 API 改动
- 数据库 migration
- widget schema 调整

## 16. 验收标准

满足以下条件即可视为本次重构达标：

- 桌面端拖拽和缩放过程中不再出现明显跳位、抖动或不可预测重排。
- 新增、删除、编辑配置不会在交互过程中破坏布局稳定性。
- 保存成功后布局与界面表现一致。
- 切换 dashboard、取消编辑、移动端切换后不会残留旧交互状态。
- 测试覆盖核心纯函数、Grid 交互边界、editor 行为和页面级流程。

## 17. 后续扩展

本次设计为以下能力预留边界，但不在当前实现范围内：

- 多断点布局
- 撤销/重做
- 局部保存
- 拖拽占位优化
- 组件级布局锁定
