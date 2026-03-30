# QR Pairing Login Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable iOS users to scan a QR code from the web dashboard to authenticate — no manual server URL or password entry needed.

**Architecture:** Server-side pair endpoints already implemented (Plan 1). This plan adds the iOS QR scanner view and the Web dashboard "Mobile Devices" management page.

**Tech Stack:** Swift (AVCaptureSession), React (TanStack Router, qrcode npm package)

**Spec:** `docs/superpowers/specs/2026-03-29-ios-mvp-design.md` Section 2 + 5.1 + 5.7

---

### Task 1: iOS — QR Scanner View

**Files:**
- Create: `apps/ios/ServerBee/Views/Auth/QRScannerView.swift`
- Modify: `apps/ios/ServerBee/Views/Auth/LoginView.swift`
- Modify: `apps/ios/ServerBee/Info.plist`

- [ ] **Step 1: Add NSCameraUsageDescription to Info.plist**

Add to `apps/ios/ServerBee/Info.plist` inside the top-level `<dict>`:

```xml
<key>NSCameraUsageDescription</key>
<string>ServerBee needs camera access to scan QR codes for quick login</string>
```

- [ ] **Step 2: Create QRScannerView**

Create `apps/ios/ServerBee/Views/Auth/QRScannerView.swift` — a SwiftUI view wrapping `AVCaptureSession` for QR code scanning. When a QR code is detected:
1. Parse JSON: `{ "type": "serverbee_pair", "server_url": "...", "code": "..." }`
2. Validate `type == "serverbee_pair"`
3. Call the completion handler with `server_url` and `code`

Use `UIViewControllerRepresentable` wrapping a `AVCaptureMetadataOutputObjectsDelegate`.

- [ ] **Step 3: Add "Scan QR Code" button to LoginView**

In `LoginView.swift`, add a button that presents `QRScannerView` as a sheet. On successful scan:
1. Set `authManager.setServerUrl(serverUrl)`
2. POST to `/api/mobile/auth/pair` with `{ code, installation_id, device_name }`
3. On success, call `authManager.handleLoginResponse(tokenResponse)`

- [ ] **Step 4: Commit**

```bash
git add apps/ios/ServerBee/Views/Auth/QRScannerView.swift apps/ios/ServerBee/Views/Auth/LoginView.swift apps/ios/ServerBee/Info.plist
git commit -m "feat(ios): add QR scanner for pairing login"
```

---

### Task 2: Web — Mobile Devices management page

**Files:**
- Create: `apps/web/src/routes/_authed/settings/mobile-devices.tsx`
- Create: `apps/web/src/components/mobile-pair-dialog.tsx`

- [ ] **Step 1: Create mobile-pair-dialog component**

A dialog that:
1. Calls `POST /api/mobile/pair` to get a pairing code
2. Generates a QR code using `qrcode` npm package (install if needed) containing JSON `{ type: "serverbee_pair", server_url: window.location.origin, code }`
3. Shows the QR with a 5-minute countdown timer
4. "Regenerate" button when expired

- [ ] **Step 2: Create mobile-devices settings page**

A route page at `/settings/mobile-devices` that:
1. Lists active mobile devices from `GET /api/mobile/auth/devices`
2. Shows device name, last active time
3. "Revoke" button per device (`DELETE /api/mobile/auth/devices/{id}`)
4. "Add Device" button opens the pair dialog

- [ ] **Step 3: Add navigation link from settings page**

Add a link to the mobile devices page from the existing settings navigation.

- [ ] **Step 4: Install qrcode dependency if needed**

```bash
cd apps/web && bun add qrcode @types/qrcode
```

- [ ] **Step 5: Commit**

```bash
git add apps/web/
git commit -m "feat(web): add mobile devices management page with QR pairing"
```
