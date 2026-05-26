# Custom SPA Themes — Manual E2E Checklist

End-to-end smoke test for the Custom Frontend Theme feature (`.sbtheme` upload, preview, activation, recovery, and deletion).

## Setup

### 1. Build the starter theme

```bash
cd templates/serverbee-theme-starter
bun install
bun run pack
# produces serverbee-theme-starter-<version>.sbtheme in the project root
cd ../..
```

### 2. Start the backend

```bash
SERVERBEE_ADMIN__PASSWORD=admin123 SERVERBEE_AUTH__SECURE_COOKIE=false cargo run -p serverbee-server
```

Default listen address: `http://localhost:9527`

### 3. Start the frontend dev server

```bash
cd apps/web
bun run dev
# Vite proxies /api/* to http://localhost:9527
```

Default dev URL: `http://localhost:5173`

### 4. Log in as admin

Open `http://localhost:5173` (or `http://localhost:9527` if using the embedded build) and log in with `admin` / `admin123`.

---

## Happy Path — Upload, Preview, Activate

- [ ] Navigate to **Settings → Appearance**.
- [ ] The **Custom Frontend** section appears at the top of the page (visible to admin only).
- [ ] Drag the `.sbtheme` file produced by `bun run pack` onto the upload card. The upload completes with a success toast.
- [ ] The uploaded theme card appears in the list with the correct name, version, and preview image (if present).
- [ ] Click **Details** on the card. The drawer opens showing the full manifest fields (id, name, version, author, description), file list, and size.

### Preview

- [ ] Click **Preview** on the card. A dialog appears explaining that preview is browser-wide for up to 15 minutes.
- [ ] Confirm. A new browser tab opens at `/?theme=preview:<uuid>`.
- [ ] The preview theme renders correctly in the new tab.
- [ ] A fixed-position banner inside the preview tab shows the remaining time countdown and an **Exit preview** button.
- [ ] The **Activate** button is not yet available (or the current active theme has not changed) — verify by checking the management tab still shows the default SPA.
- [ ] Click **Exit preview** in the banner. `POST /__system/clear-preview` is called, the preview cookie is cleared, and the tab reloads to the default SPA.

### Preview as non-admin

- [ ] Log in as a member user in a private/incognito window.
- [ ] Visit `/?theme=preview:<uuid>` directly. The server ignores the preview request for non-admins and serves the currently active theme (default SPA if none active).

### Activate

- [ ] As admin, click **Activate** on the theme card. A confirmation dialog appears with a checkbox gate.
- [ ] The checkbox reads "I understand this will replace the frontend for all users."
- [ ] The **Activate** button inside the dialog is disabled until the checkbox is checked.
- [ ] Check the checkbox and click **Activate**.
- [ ] The dialog closes and the custom frontend is now active system-wide.
- [ ] Reloading any root-level URL (`/`, `/servers`, etc.) in any tab serves the custom theme.
- [ ] The **Settings → Appearance** page (reached via `/__system/admin/spa-themes`) still shows the default SPA management UI.
- [ ] A notice banner appears on the color themes and brand settings sections reading something like "A custom frontend is active. These settings have no effect until it is deactivated."

---

## Recovery

- [ ] With a custom theme active, visit `/?theme=default`. The browser immediately receives the default SPA.
- [ ] The URL no longer contains `theme=default` (the parameter is consumed), but a recovery cookie (`sb_force_default`) has been set.
- [ ] Navigate to another route within the default SPA (e.g. `/servers`). The default SPA continues to render (cookie is sticky).
- [ ] Reload any root route in a different tab in the same browser profile. The default SPA is also served there (cookie is origin-wide).
- [ ] On the default SPA, locate and click **Exit recovery** (or visit `/?theme=active`). The `sb_force_default` cookie is cleared.
- [ ] Reloading now serves the custom theme again.

---

## Deactivate

- [ ] Navigate to **Settings → Appearance** (default SPA management page).
- [ ] Click **Deactivate** (or set active theme to none via the API: `PUT /api/settings/active-spa-theme` with `{"theme_id": null}`).
- [ ] Reloading any root-level URL now serves the default built-in SPA.
- [ ] The custom theme card still appears in the list (deactivation does not delete).

---

## Delete

- [ ] Ensure the theme is not currently active (deactivate first if needed).
- [ ] Click **Delete** on the theme card. A confirmation dialog appears.
- [ ] Confirm deletion. The card disappears from the list.
- [ ] `GET /api/settings/spa-themes` no longer returns the deleted uuid.

---

## Error Paths

### Delete active theme

- [ ] Activate a theme.
- [ ] Attempt to delete it (via the UI Delete button or `DELETE /api/settings/spa-themes/:uuid`).
- [ ] The server returns HTTP 409 with `{"error":{"code":"THEME_IN_USE",...}}`.
- [ ] The UI shows an error message explaining the theme must be deactivated first.

### Oversize upload

- [ ] Create a zip file whose total uncompressed content exceeds 20 MB (or whose multipart body exceeds 25 MB).
- [ ] Attempt to upload it.
- [ ] For a body > 25 MB: the server returns HTTP 413 with a JSON body `{"error":{"code":"UPLOAD_TOO_LARGE",...}}` (not Axum's default plain-text response).
- [ ] For content > 20 MB but multipart < 25 MB: the server returns HTTP 400 with `{"error":{"code":"TOTAL_SIZE_EXCEEDED",...}}`.
- [ ] The UI displays the error code or message to the user.

### Zip-slip rejection

- [ ] Craft or obtain a zip that contains an entry with a path like `../../etc/passwd`.
- [ ] Attempt to upload it.
- [ ] The server returns HTTP 400 with `{"error":{"code":"ZIP_SLIP",...}}`.

### Zip-bomb rejection

- [ ] Craft or obtain a zip entry whose decompressed size exceeds its compressed size by more than 100×.
- [ ] Attempt to upload it.
- [ ] The server returns HTTP 400 with `{"error":{"code":"ZIP_BOMB",...}}`.

### Version downgrade

- [ ] Upload a theme at version `1.1.0` (or use the starter and bump the version).
- [ ] Attempt to upload the same theme `id` at version `1.0.0`.
- [ ] The server returns HTTP 400 with `{"error":{"code":"NO_DOWNGRADE",...}}`.

### Same version already exists

- [ ] Upload a theme at version `1.0.0`.
- [ ] Attempt to upload the same `id` + `version` again.
- [ ] The server returns HTTP 409 with `{"error":{"code":"VERSION_EXISTS",...}}`.

### Version upgrade (succeeds)

- [ ] Upload theme `id` at version `1.0.0`.
- [ ] Upload the same `id` at version `1.1.0`.
- [ ] The upload succeeds; the response includes `is_upgrade_of` pointing to the v1.0.0 uuid.
- [ ] The v1.0.0 row is marked as superseded (visible in the details drawer or API response).
- [ ] Neither version is automatically activated — the active theme remains unchanged.

---

## CSP Headers

- [ ] With a custom theme active, open DevTools → Network and inspect any top-level HTML response (e.g. `GET /`).
- [ ] The response includes a `Content-Security-Policy` header with `connect-src 'self'`.
- [ ] Inspect a root HTML response while the default SPA is active — no `Content-Security-Policy` header is present (the default SPA does not set CSP).

---

## SPA History Routing

- [ ] With a custom theme active (built from the starter with `base: '/'`), navigate to a deep path within the custom SPA (e.g. `/servers/abc`).
- [ ] Press F5 (hard reload) on that deep path.
- [ ] The custom theme `index.html` is served (HTTP 200) and the SPA re-initializes correctly — no 404 or broken asset paths.

---

## Audit Log

Navigate to **Settings → Audit Logs** and verify an entry exists for each of the following actions after performing them:

- [ ] After uploading a theme: an entry with `action = "spa_theme.upload"` and detail containing the uuid, manifest_id, version, and size_bytes.
- [ ] After activating a theme: an entry with `action = "spa_theme.activate"` and detail containing the uuid.
- [ ] After deactivating a theme: an entry with `action = "spa_theme.deactivate"` and detail containing the previous active uuid.
- [ ] After deleting a theme: an entry with `action = "spa_theme.delete"` and detail containing the uuid, manifest_id, and version.

---

## Color/Brand Disabled Banner

- [ ] With a custom frontend theme active, navigate to **Settings → Appearance** (via `/__system/admin/spa-themes` or the default SPA).
- [ ] The color theme grid section displays a notice such as "A custom frontend is active. These settings have no effect until it is deactivated."
- [ ] The brand settings section displays the same or a similar notice.
- [ ] After deactivating the custom frontend, reload the appearance page. The notices are gone and the color theme grid and brand settings are fully interactive again.

---

## Non-admin Access

- [ ] Log in as a member user (non-admin).
- [ ] Navigate to **Settings → Appearance**.
- [ ] The **Custom Frontend** section is not visible.
- [ ] Direct API calls from the member session return 403:
  - `POST /api/settings/spa-themes`
  - `DELETE /api/settings/spa-themes/:uuid`
  - `PUT /api/settings/active-spa-theme`
