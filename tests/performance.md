# 前端性能测试

## 概述

Server 详情页 realtime 模式下有 7 个 Recharts 图表随 WebSocket 数据实时更新，是性能热点。

## 优化措施

| 措施 | 文件 | 说明 |
|------|------|------|
| 渲染节流 | `hooks/use-realtime-metrics.ts` | `RENDER_THROTTLE_MS=2000`，限制最多每 2 秒触发一次 re-render |
| 缩短动画 | `components/server/metrics-chart.tsx` | `animationDuration={800}`（默认 1500ms），避免与 2s 更新周期叠加 |
| 缩短动画 | `components/server/disk-io-chart.tsx` | 同上 |

---

## 使用 agent-browser 测量性能

```bash
# 1. 登录并导航到 server 详情 realtime 页面
agent-browser open http://localhost:5173/login
agent-browser snapshot -i
agent-browser fill @e5 "admin" && agent-browser fill @e6 "admin123" && agent-browser click @e4
agent-browser wait --load networkidle
agent-browser open "http://localhost:5173/servers/<SERVER_ID>?range=realtime"
agent-browser wait --load networkidle

# 2. 测量 DOM mutations + Long Tasks（10 秒）
agent-browser eval --stdin <<'EVALEOF'
(function() {
  return new Promise(resolve => {
    let domMutations = 0, longTasks = 0, longTaskDurations = [];
    const observer = new MutationObserver(m => { domMutations += m.length; });
    observer.observe(document.body, { childList: true, subtree: true, attributes: true });
    const perfObserver = new PerformanceObserver(list => {
      for (const e of list.getEntries()) { longTasks++; longTaskDurations.push(Math.round(e.duration)); }
    });
    try { perfObserver.observe({ entryTypes: ['longtask'] }); } catch(e) {}
    setTimeout(() => {
      observer.disconnect(); perfObserver.disconnect();
      resolve(JSON.stringify({ domMutations, longTasks, longTaskDurations }));
    }, 10000);
  });
})()
EVALEOF

# 3. 测量 FPS（10 秒）
agent-browser eval --stdin <<'EVALEOF'
(function() {
  return new Promise(resolve => {
    let frames = 0, lastTime = performance.now(), fpsReadings = [];
    function tick() {
      frames++;
      const now = performance.now();
      if (now - lastTime >= 1000) {
        fpsReadings.push(Math.round(frames * 1000 / (now - lastTime)));
        frames = 0; lastTime = now;
      }
      if (fpsReadings.length < 10) requestAnimationFrame(tick);
    }
    requestAnimationFrame(tick);
    setTimeout(() => {
      const avg = fpsReadings.length ? Math.round(fpsReadings.reduce((a,b) => a+b, 0) / fpsReadings.length) : 0;
      resolve(JSON.stringify({ avgFps: avg, minFps: Math.min(...fpsReadings), maxFps: Math.max(...fpsReadings), fpsPerSecond: fpsReadings }));
    }, 10500);
  });
})()
EVALEOF

# 4. 清理
agent-browser close
```

---

## 性能基准（2026-03-21）

测试环境：Server 详情页 realtime 模式，7 个图表，1920×963 视口

| 指标 | 数值 | 阈值 |
|------|------|------|
| DOM mutations / 10s | 2469 | < 5000 |
| Long tasks | 3 次（68/72/69ms） | 每个 < 100ms |
| 平均 FPS | 62 | > 50 |
| 最低 FPS | 50 | > 30 |
| 内存 (JS Heap) | 37→50 MB / 10s | < 200 MB |

---

## 性能回归判断标准

- **FPS 平均值 < 30**：需要优化
- **Long task > 200ms**：需要排查
- **DOM mutations / 10s > 10000**：可能有动画叠加或缺少节流
