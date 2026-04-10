# Dev Proxy to Production — Manual Verification Checklist

Related spec: `docs/superpowers/specs/2026-04-10-dev-proxy-to-production-design.md`  
Related plan: `docs/superpowers/plans/2026-04-10-dev-proxy-to-production.md`

Use this checklist when changing any of:

- `apps/web/vite/dev-proxy.ts`
- `apps/web/vite.config.ts`
- `apps/web/src/components/dev-proxy-banner.tsx`
- `apps/web/src/routes/__root.tsx`

Automated tests cover the proxy factory and banner logic. This checklist covers end-to-end behavior that needs a real browser and real network traffic.

## 前置条件

1. Ensure the project root `.env` has:
   - `SERVERBEE_PROD_URL=https://<your-prod>.up.railway.app`
   - `SERVERBEE_PROD_READONLY_API_KEY=<member-role key>`
   - Optional: `SERVERBEE_PROD_API_KEY=<admin key>` for `make db-pull`
2. The read-only key must use role `member`, not `admin`.
3. Start from a clean browser session so old localhost cookies do not muddy the picture.

## 测试项

### 1. 默认本地模式不受影响

- [ ] Run `make dev` or `make web-dev`.
- [ ] Confirm the app boots normally against local services.
- [ ] Confirm no prod warning banner is visible.
- [ ] Confirm the server list still loads from local `:9527`.

### 2. 正常路径，实时生产数据

- [ ] Run `make web-dev-prod`.
- [ ] Confirm Vite starts without env validation errors.
- [ ] Open `http://localhost:5173/`.
- [ ] Confirm the banner shows `⚠ Dev proxy → PROD (https://...) · read-only`.
- [ ] Confirm the server list loads production data.
- [ ] Confirm charts or server status update in real time, proving `/api/ws/servers` proxying works.
- [ ] Confirm control-plane WebSocket routes such as terminal or Docker log streaming do not connect through prod-proxy mode.

### 3. 默认写入拦截

- [ ] In prod-proxy mode, attempt a harmless write such as saving a setting or changing a row that you can safely revert.
- [ ] Confirm DevTools shows a `403` response from the proxy layer.
- [ ] Confirm the response body contains the read-only message.
- [ ] Refresh and confirm production state did not change.

### 4. 可选，显式开启写入

- [ ] Stop the dev server.
- [ ] Re-run with `ALLOW_WRITES=1 make web-dev-prod`.
- [ ] Confirm the banner no longer claims read-only. It should switch to the stronger `WRITE ACCESS ENABLED` warning.
- [ ] Retry a harmless write.
- [ ] Confirm the request now reaches production. A backend success or backend `403` are both acceptable here, the point is that the proxy layer no longer blocks the method.

### 5. 认证隔离

- [ ] Navigate to `/login`.
- [ ] Attempt to sign in with any credentials.
- [ ] Confirm `POST /api/auth/login` is blocked.
- [ ] Confirm proxied responses do not expose `Set-Cookie`.
- [ ] Confirm `GET /api/auth/me` still works for showing the current user banner state.

### 6. 缺失环境变量时的报错

- [ ] Temporarily remove or rename the project root `.env`.
- [ ] Run `make web-dev-prod`.
- [ ] Confirm startup fails with an error naming `SERVERBEE_PROD_URL` and pointing at the repo root `.env`.
- [ ] Restore `.env`.
- [ ] Temporarily remove `SERVERBEE_PROD_READONLY_API_KEY`.
- [ ] Run `make web-dev-prod`.
- [ ] Confirm startup fails with an error naming `SERVERBEE_PROD_READONLY_API_KEY` and warning not to reuse `SERVERBEE_PROD_API_KEY`.
- [ ] Restore `.env`.

### 7. 已知危险配置，文档化失败模式

- [ ] Set `SERVERBEE_PROD_READONLY_API_KEY` to the same value as `SERVERBEE_PROD_API_KEY`.
- [ ] Run `ALLOW_WRITES=1 make web-dev-prod`.
- [ ] Perform a harmless write.
- [ ] Confirm the write can now reach production with admin scope.
- [ ] Restore the correct member key immediately after the check.

This is not a success case. It documents the failure mode so nobody gets cute and then acts surprised when production gets poked.

### 8. 视觉检查

- [ ] Confirm the banner is readable in light theme.
- [ ] Confirm the banner is readable in dark theme.
- [ ] Confirm the banner stays fixed while scrolling.
- [ ] Confirm the banner does not intercept clicks.
- [ ] Confirm page content remains usable with the banner visible.

## 通过标准

Sections 1, 2, 3, 5, and 6 should pass for the feature to be considered healthy. Section 4 is optional because it intentionally enables a riskier local override. Section 7 is a documented unsafe configuration check, not a normal pass condition. Section 8 is visual QA and should be revisited whenever the banner styling or layout changes.
