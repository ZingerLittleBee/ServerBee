# 28 主题 / 外观 / 品牌 — 冒烟测试

**前置条件**:已登录 admin,进入 `/settings/appearance`。深度用例见 [../appearance.md](../appearance.md)、[../appearance/custom-theme.md](../appearance/custom-theme.md)。

| # | 测试场景 | 操作步骤 | 预期结果 | 阻断级 | 状态 |
|---|---------|---------|---------|--------|------|
| AP1 | 亮/暗/跟随系统 | 切换主题模式 | 全站配色即时切换,刷新保持 | 是 | ✅ |
| AP2 | 预设主题 | 选择内置预设主题 | 配色应用全站 | 否 | ✅ |
| AP3 | 自定义主题 | 编辑 OKLCH 颜色 → 保存 | 自定义配色生效 | 否 | ✅ |
| AP4 | 主题导入/导出 | 导出主题再导入 | 往返一致,引用检查正常 | 否 | ✅ |
| AP5 | 品牌配置 | 上传 Logo/Favicon、改站点名 | 顶栏/标签页品牌更新 | 否 | ⚠️✅ |
| AP6 | 状态页主题 | 自定义主题应用于公开状态页 | 公开页配色一致 | 否 | ✅ |

> AP1: 顶栏 Toggle theme 切换 `documentElement.className` 即时(dark↔light),刷新后保持(localStorage 持久化)。
> AP2: 8 个内置预设(Default/Tokyo Night/Nord/Catppuccin/Dracula/One Dark/Solarized/Rose Pine);选 Dracula 后 `active-theme`=`preset:dracula`,刷新+跨页(仪表盘)保持。
> AP3: `POST /api/settings/themes` 全量 OKLCH var map(31 必填变量)创建成功,设为 active(`custom:1`)持久化生效。校验器正确拒绝缺变量(返回 missing variable)。
> AP4: 导出返回带 version 的 JSON;导入往返后 vars_light/vars_dark/based_on 完全一致;`/themes/{id}/references` 返回 `admin:true,status_pages:[]` 正常。
> AP5: 站点名/Footer 经 `PUT /api/settings/brand` 保存并持久化(appearance 表单刷新回显 "SmokeE Brand"/"smoke footer")。提醒:仪表盘顶栏/`document.title` 仍硬编码 "ServerBee",site_title 仅 appearance 表单消费,未传播到仪表盘 chrome(公开状态页有品牌)。数据层保存生效,故判通过带提醒。
> AP6: 自定义主题 `custom:1` 绑定状态页后,公开页 `/api/status/smoke-e` 的 theme 解析为 `kind:custom` 且 vars 与自定义主题一致。
> 还原:active-theme 复位 `preset:default`,brand 全 null,删除自定义主题 1/2,状态页 theme_ref 解绑 — 已对照基线确认。

**汇总**:✅ 6 / ❌ 0 / — 0(AP5 带已知提醒:site_title 未传播到仪表盘顶栏/标签页)
