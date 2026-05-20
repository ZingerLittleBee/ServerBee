# iOS Plan 4: Login, Camera, and Language

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a production-grade login experience: a safely-configured QR scanner with proper permission flow, an actual user-controllable device name, an honest language story (rely on iOS Settings), a consolidated pair flow through the ViewModel, and keyboard-safe TOTP entry on small screens.

**Architecture:** `QRScannerViewController` runs all capture-session mutations on a dedicated serial queue with proper configuration brackets. Device name is a user-editable Settings field with a stable suffix. Language switching is delegated to iOS per-app Settings (in-app Picker removed). Pair flow lives in `AuthViewModel` so the View is presentational.

**Tech Stack:** SwiftUI, AVFoundation, XCTest.

**Depends on:** Plan 1 (`ServerBeeTests` target), Plan 2 (AuthManager `@MainActor`).

---

## File Structure

**Modified files:**
- `apps/ios/ServerBee/Views/Auth/QRScannerView.swift` — rewrite session lifecycle on serial queue; add permission UI.
- `apps/ios/ServerBee/Views/Auth/LoginView.swift` — delegate pair to ViewModel; keyboard avoidance for TOTP.
- `apps/ios/ServerBee/ViewModels/AuthViewModel.swift` — own `pair(serverUrl:code:)`; use stored device name.
- `apps/ios/ServerBee/Views/Settings/AppearanceView.swift` — drop language Picker; document decision.
- `apps/ios/ServerBee/Views/Settings/SettingsView.swift` — add Device Name row.
- `apps/ios/ServerBee/Info.plist` — add `CFBundleLocalizations`.
- `apps/ios/project.yml` — ensure `CFBundleAllowMixedLocalizations` is set.

**Created files:**
- `apps/ios/ServerBee/Services/DeviceNameProvider.swift` — stored device-name + random suffix utility.
- `apps/ios/ServerBee/Views/Settings/DeviceNameRow.swift` — Settings row for editing device name.
- `apps/ios/ServerBeeTests/DeviceNameProviderTests.swift` — persistence + suffix tests.
- `apps/ios/ServerBeeTests/AuthViewModelPairTests.swift` — pair flow tests via URLProtocolStub.

---

## Backend Reference (verified)

`POST /api/mobile/auth/pair` (see `crates/server/src/router/api/mobile.rs:323-381`) returns:
- `200` — success, body is `ApiResponse<MobileTokenResponse>`
- `400` — invalid/expired pairing code or user not found
- `422` — generic validation error (missing `code`/`installation_id`/`device_name`); **no 2FA branch on this route**
- `429` — handled by middleware

Pair flow therefore does NOT have a TOTP step. 422 must be surfaced as a generic validation error, not as "show TOTP".

---

## Task 1: Refactor `QRScannerViewController` to use a serial capture queue

**Files:**
- Modify: `apps/ios/ServerBee/Views/Auth/QRScannerView.swift` (entire file)

- [ ] **Step 1: Replace the file contents** with the safe-session implementation. Configuration is bracketed by `beginConfiguration()/commitConfiguration()` and all session mutations run on the serial `captureQueue`. The SwiftUI body and permission UI are added in Task 2.

```swift
import AVFoundation
import SwiftUI
import UIKit

struct QRScannerView: UIViewControllerRepresentable {
    let onScanned: (String, String) -> Void
    @Environment(\.dismiss) private var dismiss

    func makeUIViewController(context: Context) -> QRScannerViewController {
        let controller = QRScannerViewController()
        controller.onScanned = { serverUrl, code in
            onScanned(serverUrl, code)
        }
        controller.onDismiss = {
            dismiss()
        }
        return controller
    }

    func updateUIViewController(_ uiViewController: QRScannerViewController, context: Context) {}
}

// MARK: - QR Scanner View Controller

final class QRScannerViewController: UIViewController, @preconcurrency AVCaptureMetadataOutputObjectsDelegate {
    var onScanned: ((String, String) -> Void)?
    var onDismiss: (() -> Void)?

    /// Serial queue that owns ALL `AVCaptureSession` mutations. Per Apple's
    /// AVFoundation docs, `beginConfiguration()`, `addInput`, `addOutput`,
    /// `startRunning()`, and `stopRunning()` must not be interleaved across
    /// threads. Routing everything through a single serial queue gives us
    /// that ordering guarantee without `nonisolated(unsafe)`.
    private let captureQueue = DispatchQueue(label: "com.serverbee.capture", qos: .userInitiated)
    private let captureSession = AVCaptureSession()
    private var previewLayer: AVCaptureVideoPreviewLayer?
    private var hasScanned = false
    private var isConfigured = false

    /// Permission state observed by the SwiftUI wrapper. Updated on the main
    /// actor so the host view can swap to a "denied" panel.
    enum PermissionState: Equatable {
        case unknown
        case authorized
        case denied
    }

    @Published private(set) var permissionState: PermissionState = .unknown

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        setupDismissButton()
        requestCameraAndStart()
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        previewLayer?.frame = view.bounds
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        stopSession()
    }

    // MARK: - Permission + start

    private func requestCameraAndStart() {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized:
            permissionState = .authorized
            configureAndStartSession()
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { [weak self] granted in
                DispatchQueue.main.async {
                    guard let self else { return }
                    if granted {
                        self.permissionState = .authorized
                        self.configureAndStartSession()
                    } else {
                        self.permissionState = .denied
                        self.showPermissionDeniedUI()
                    }
                }
            }
        case .denied, .restricted:
            permissionState = .denied
            showPermissionDeniedUI()
        @unknown default:
            permissionState = .denied
            showPermissionDeniedUI()
        }
    }

    // MARK: - Session lifecycle (serial queue only)

    private func configureAndStartSession() {
        captureQueue.async { [weak self] in
            guard let self else { return }
            self.configureSessionLocked()
            if !self.captureSession.isRunning {
                self.captureSession.startRunning()
            }
        }
    }

    /// Must run on `captureQueue`. Configures inputs/outputs exactly once.
    private func configureSessionLocked() {
        guard !isConfigured else { return }

        captureSession.beginConfiguration()
        defer { captureSession.commitConfiguration() }

        guard let device = AVCaptureDevice.default(for: .video) else {
            DispatchQueue.main.async { [weak self] in
                self?.showError(String(localized: "qr_camera_unavailable"))
            }
            return
        }
        guard let input = try? AVCaptureDeviceInput(device: device),
              captureSession.canAddInput(input)
        else {
            DispatchQueue.main.async { [weak self] in
                self?.showError(String(localized: "qr_camera_input_failed"))
            }
            return
        }
        captureSession.addInput(input)

        let metadataOutput = AVCaptureMetadataOutput()
        guard captureSession.canAddOutput(metadataOutput) else {
            DispatchQueue.main.async { [weak self] in
                self?.showError(String(localized: "qr_camera_output_failed"))
            }
            return
        }
        captureSession.addOutput(metadataOutput)
        metadataOutput.setMetadataObjectsDelegate(self, queue: DispatchQueue.main)
        metadataOutput.metadataObjectTypes = [.qr]

        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            let preview = AVCaptureVideoPreviewLayer(session: self.captureSession)
            preview.frame = self.view.bounds
            preview.videoGravity = .resizeAspectFill
            self.view.layer.insertSublayer(preview, at: 0)
            self.previewLayer = preview
        }

        isConfigured = true
    }

    private func stopSession() {
        captureQueue.async { [weak self] in
            guard let self else { return }
            if self.captureSession.isRunning {
                self.captureSession.stopRunning()
            }
        }
    }

    // MARK: - Dismiss

    private func setupDismissButton() {
        let dismissButton = UIButton(type: .system)
        let config = UIImage.SymbolConfiguration(pointSize: 22, weight: .medium)
        dismissButton.setImage(UIImage(systemName: "xmark.circle.fill", withConfiguration: config), for: .normal)
        dismissButton.tintColor = .white
        dismissButton.addTarget(self, action: #selector(dismissTapped), for: .touchUpInside)
        dismissButton.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(dismissButton)

        NSLayoutConstraint.activate([
            dismissButton.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 16),
            dismissButton.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -16),
            dismissButton.widthAnchor.constraint(equalToConstant: 44),
            dismissButton.heightAnchor.constraint(equalToConstant: 44),
        ])
    }

    @objc private func dismissTapped() {
        onDismiss?()
    }

    // MARK: - AVCaptureMetadataOutputObjectsDelegate

    func metadataOutput(
        _ output: AVCaptureMetadataOutput,
        didOutput metadataObjects: [AVMetadataObject],
        from connection: AVCaptureConnection
    ) {
        guard !hasScanned,
              let metadataObject = metadataObjects.first as? AVMetadataMachineReadableCodeObject,
              metadataObject.type == .qr,
              let stringValue = metadataObject.stringValue
        else {
            return
        }

        guard let data = stringValue.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let type = json["type"] as? String,
              type == "serverbee_pair",
              let serverUrl = json["server_url"] as? String,
              let code = json["code"] as? String
        else {
            return
        }

        hasScanned = true
        stopSession()

        AudioServicesPlaySystemSound(SystemSoundID(kSystemSoundID_Vibrate))
        onScanned?(serverUrl, code)
    }

    // MARK: - Error + permission UI

    fileprivate func showPermissionDeniedUI() {
        // Clear any previous transient label.
        view.subviews.filter { $0.tag == 9001 }.forEach { $0.removeFromSuperview() }

        let container = UIStackView()
        container.tag = 9001
        container.axis = .vertical
        container.alignment = .center
        container.spacing = 12
        container.translatesAutoresizingMaskIntoConstraints = false

        let title = UILabel()
        title.text = String(localized: "qr_permission_denied_title")
        title.font = .systemFont(ofSize: 18, weight: .semibold)
        title.textColor = .white
        title.textAlignment = .center
        title.numberOfLines = 0

        let body = UILabel()
        body.text = String(localized: "qr_permission_denied_body")
        body.font = .systemFont(ofSize: 14)
        body.textColor = .white.withAlphaComponent(0.85)
        body.textAlignment = .center
        body.numberOfLines = 0

        let openSettings = UIButton(type: .system)
        openSettings.setTitle(String(localized: "qr_open_settings"), for: .normal)
        openSettings.titleLabel?.font = .systemFont(ofSize: 16, weight: .semibold)
        openSettings.addTarget(self, action: #selector(openSettingsTapped), for: .touchUpInside)

        container.addArrangedSubview(title)
        container.addArrangedSubview(body)
        container.addArrangedSubview(openSettings)
        view.addSubview(container)

        NSLayoutConstraint.activate([
            container.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            container.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            container.leadingAnchor.constraint(greaterThanOrEqualTo: view.leadingAnchor, constant: 24),
            container.trailingAnchor.constraint(lessThanOrEqualTo: view.trailingAnchor, constant: -24),
        ])
    }

    @objc private func openSettingsTapped() {
        guard let url = URL(string: UIApplication.openSettingsURLString) else { return }
        UIApplication.shared.open(url)
    }

    private func showError(_ message: String) {
        let label = UILabel()
        label.text = message
        label.textColor = .white
        label.textAlignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(label)

        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: view.centerYAnchor),
        ])
    }
}
```

- [ ] **Step 2: Add the localized strings** the new file references. Open `apps/ios/ServerBee/Localizable.xcstrings` and add the following keys (each with `en` and `zh-Hans` translations). Insert anywhere in the `strings` object:

```json
"qr_camera_unavailable" : {
  "extractionState" : "manual",
  "localizations" : {
    "en"      : { "stringUnit" : { "state" : "translated", "value" : "Camera not available" } },
    "zh-Hans" : { "stringUnit" : { "state" : "translated", "value" : "摄像头不可用" } }
  }
},
"qr_camera_input_failed" : {
  "extractionState" : "manual",
  "localizations" : {
    "en"      : { "stringUnit" : { "state" : "translated", "value" : "Cannot access camera" } },
    "zh-Hans" : { "stringUnit" : { "state" : "translated", "value" : "无法访问摄像头" } }
  }
},
"qr_camera_output_failed" : {
  "extractionState" : "manual",
  "localizations" : {
    "en"      : { "stringUnit" : { "state" : "translated", "value" : "Cannot configure scanner" } },
    "zh-Hans" : { "stringUnit" : { "state" : "translated", "value" : "无法配置扫码器" } }
  }
},
"qr_permission_denied_title" : {
  "extractionState" : "manual",
  "localizations" : {
    "en"      : { "stringUnit" : { "state" : "translated", "value" : "Camera permission denied" } },
    "zh-Hans" : { "stringUnit" : { "state" : "translated", "value" : "摄像头权限被拒绝" } }
  }
},
"qr_permission_denied_body" : {
  "extractionState" : "manual",
  "localizations" : {
    "en"      : { "stringUnit" : { "state" : "translated", "value" : "ServerBee needs camera access to scan QR codes. Enable it in Settings." } },
    "zh-Hans" : { "stringUnit" : { "state" : "translated", "value" : "ServerBee 需要摄像头权限才能扫描二维码。请在系统设置中开启。" } }
  }
},
"qr_open_settings" : {
  "extractionState" : "manual",
  "localizations" : {
    "en"      : { "stringUnit" : { "state" : "translated", "value" : "Open Settings" } },
    "zh-Hans" : { "stringUnit" : { "state" : "translated", "value" : "打开设置" } }
  }
}
```

- [ ] **Step 3: Compile**

Run:
```bash
cd apps/ios && xcodegen generate && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee -destination 'generic/platform=iOS Simulator' -configuration Debug build -quiet
```
Expected: BUILD SUCCEEDED.

- [ ] **Step 4: Commit**

```bash
git add apps/ios/ServerBee/Views/Auth/QRScannerView.swift apps/ios/ServerBee/Localizable.xcstrings
git commit -m "fix(ios): serialize QR capture session and add permission UI"
```

---

## Task 2: Manual smoke test for the camera permission flow

**Files:** none (manual verification only).

- [ ] **Step 1: Reset simulator privacy**

Run:
```bash
xcrun simctl privacy booted reset camera com.serverbee.mobile
```
Expected: command exits 0.

- [ ] **Step 2: Launch the app and trigger the scanner**

Run the app on the simulator, tap **Login → Scan QR Code**.
Expected: iOS shows the camera permission prompt with the description from `Info.plist`. Tapping **Allow** dismisses the prompt and the camera preview appears within ~1 second.

- [ ] **Step 3: Deny path**

Run:
```bash
xcrun simctl privacy booted revoke camera com.serverbee.mobile
```
Re-open the scanner.
Expected: the view shows the "Camera permission denied" title, body, and an **Open Settings** button that opens the ServerBee privacy page.

- [ ] **Step 4: No commit** — record the result in PR description; manual-only step.

---

## Task 3: Create `DeviceNameProvider` with stable random suffix

**Files:**
- Create: `apps/ios/ServerBee/Services/DeviceNameProvider.swift`

- [ ] **Step 1: Write the provider**

```swift
import Foundation
import UIKit

/// Generates and persists a user-editable device name for the mobile pair/login
/// flow. On iOS 16+, `UIDevice.current.name` returns the model literal (e.g.,
/// "iPhone") for apps without the `com.apple.developer.device-information.
/// user-assigned-device-name` entitlement, which makes every device's
/// `device_name` identical on the server side. This provider gives each
/// installation a stable, distinguishable name composed of model + iOS version
/// + a random 4-character suffix that is generated exactly once.
enum DeviceNameProvider {
    private static let storageKey = "deviceName"
    private static let suffixKey = "deviceNameSuffix"

    /// Returns the user-customised name if set, otherwise the auto-generated
    /// default. Always non-empty.
    static func current(defaults: UserDefaults = .standard) -> String {
        if let stored = defaults.string(forKey: storageKey), !stored.isEmpty {
            return stored
        }
        return defaultName(defaults: defaults)
    }

    /// Persists a user-chosen name. Empty strings are coerced to the default.
    static func set(_ name: String, defaults: UserDefaults = .standard) {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            defaults.removeObject(forKey: storageKey)
        } else {
            defaults.set(trimmed, forKey: storageKey)
        }
    }

    /// The auto-generated default. Stable across calls because the suffix is
    /// persisted on first read.
    static func defaultName(defaults: UserDefaults = .standard) -> String {
        let suffix = stableSuffix(defaults: defaults)
        let model = UIDevice.current.model
        let version = UIDevice.current.systemVersion
        return "\(model) \(version) (\(suffix))"
    }

    private static func stableSuffix(defaults: UserDefaults) -> String {
        if let existing = defaults.string(forKey: suffixKey), existing.count == 4 {
            return existing
        }
        let alphabet = Array("ABCDEFGHJKLMNPQRSTUVWXYZ23456789")
        let generated = String((0 ..< 4).map { _ in alphabet.randomElement()! })
        defaults.set(generated, forKey: suffixKey)
        return generated
    }
}
```

- [ ] **Step 2: Compile**

Run:
```bash
cd apps/ios && xcodegen generate && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee -destination 'generic/platform=iOS Simulator' -configuration Debug build -quiet
```
Expected: BUILD SUCCEEDED.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Services/DeviceNameProvider.swift
git commit -m "feat(ios): add DeviceNameProvider with stable suffix"
```

---

## Task 4: Test `DeviceNameProvider` (red → green → commit)

**Files:**
- Create: `apps/ios/ServerBeeTests/DeviceNameProviderTests.swift`

- [ ] **Step 1: Write the failing tests**

```swift
import XCTest
@testable import ServerBee

final class DeviceNameProviderTests: XCTestCase {
    private var defaults: UserDefaults!
    private let suiteName = "DeviceNameProviderTests"

    override func setUp() {
        super.setUp()
        UserDefaults().removePersistentDomain(forName: suiteName)
        defaults = UserDefaults(suiteName: suiteName)
    }

    override func tearDown() {
        UserDefaults().removePersistentDomain(forName: suiteName)
        defaults = nil
        super.tearDown()
    }

    func test_defaultName_isNonEmpty_andContainsFourCharSuffix() {
        let name = DeviceNameProvider.defaultName(defaults: defaults)
        XCTAssertFalse(name.isEmpty)
        // Suffix is wrapped in parentheses at the end: "Model 17.0 (AB12)".
        guard let open = name.lastIndex(of: "("),
              let close = name.lastIndex(of: ")"),
              open < close
        else {
            XCTFail("Default name missing (suffix): \(name)")
            return
        }
        let suffix = name[name.index(after: open) ..< close]
        XCTAssertEqual(suffix.count, 4)
    }

    func test_suffix_isStableAcrossCalls() {
        let first = DeviceNameProvider.defaultName(defaults: defaults)
        let second = DeviceNameProvider.defaultName(defaults: defaults)
        XCTAssertEqual(first, second)
    }

    func test_set_persistsCustomName() {
        DeviceNameProvider.set("My iPhone", defaults: defaults)
        XCTAssertEqual(DeviceNameProvider.current(defaults: defaults), "My iPhone")
    }

    func test_set_emptyString_fallsBackToDefault() {
        DeviceNameProvider.set("My iPhone", defaults: defaults)
        DeviceNameProvider.set("   ", defaults: defaults)
        let value = DeviceNameProvider.current(defaults: defaults)
        XCTAssertTrue(value.contains(UIDevice.current.model))
    }

    func test_current_returnsDefaultWhenUnset() {
        let value = DeviceNameProvider.current(defaults: defaults)
        XCTAssertEqual(value, DeviceNameProvider.defaultName(defaults: defaults))
    }
}
```

- [ ] **Step 2: Run tests — expect failure** (file not yet wired into target if `ServerBeeTests` was just added by Plan 1; xcodegen needs to pick it up).

Run:
```bash
cd apps/ios && xcodegen generate && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' test -only-testing:ServerBeeTests/DeviceNameProviderTests -quiet
```
Expected on first run before regeneration: target/test missing. After `xcodegen generate`, tests should run and all 5 pass (since the implementation from Task 3 is complete). If any test reports compile errors, fix and re-run.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBeeTests/DeviceNameProviderTests.swift
git commit -m "test(ios): cover DeviceNameProvider persistence and suffix"
```

---

## Task 5: Replace `UIDevice.current.name` callers with `DeviceNameProvider`

**Files:**
- Modify: `apps/ios/ServerBee/ViewModels/AuthViewModel.swift:39`
- Modify: `apps/ios/ServerBee/Views/Auth/LoginView.swift:121`

- [ ] **Step 1: Patch `AuthViewModel.login(...)`**

In `AuthViewModel.swift`, replace the `deviceName: UIDevice.current.name,` line inside the `MobileLoginRequest` initializer with:

```swift
            deviceName: DeviceNameProvider.current(),
```

So lines 35-41 become:

```swift
        let loginRequest = MobileLoginRequest(
            username: username,
            password: password,
            installationId: installationId,
            deviceName: DeviceNameProvider.current(),
            totpCode: step == .totp ? totpCode : nil
        )
```

- [ ] **Step 2: Patch `LoginView.pair(...)`**

In `LoginView.swift`, replace the body dictionary on lines 118-122 with:

```swift
        let body: [String: String] = [
            "code": code,
            "installation_id": InstallationID.getOrCreate(),
            "device_name": DeviceNameProvider.current(),
        ]
```

(This call site is removed entirely in Task 8 when pair migrates to the ViewModel. The interim replacement here keeps the diff minimal in case Task 8 is skipped or rolled back.)

- [ ] **Step 3: Build**

Run:
```bash
cd apps/ios && xcodegen generate && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee -destination 'generic/platform=iOS Simulator' -configuration Debug build -quiet
```
Expected: BUILD SUCCEEDED.

- [ ] **Step 4: Commit**

```bash
git add apps/ios/ServerBee/ViewModels/AuthViewModel.swift apps/ios/ServerBee/Views/Auth/LoginView.swift
git commit -m "fix(ios): use stored device name instead of UIDevice.current.name"
```

---

## Task 6: Add a Device Name row to Settings

**Files:**
- Create: `apps/ios/ServerBee/Views/Settings/DeviceNameRow.swift`
- Modify: `apps/ios/ServerBee/Views/Settings/SettingsView.swift` (insert under `accountSection`)

- [ ] **Step 1: Create the row**

```swift
import SwiftUI

struct DeviceNameRow: View {
    @State private var draft: String = DeviceNameProvider.current()
    @FocusState private var focused: Bool

    var body: some View {
        HStack {
            Text(String(localized: "settings_device_name"))
                .font(.body)
            Spacer()
            TextField(
                DeviceNameProvider.defaultName(),
                text: $draft
            )
            .multilineTextAlignment(.trailing)
            .textInputAutocapitalization(.words)
            .autocorrectionDisabled()
            .focused($focused)
            .submitLabel(.done)
            .onSubmit { commit() }
            .onChange(of: focused) { _, isFocused in
                if !isFocused { commit() }
            }
        }
    }

    private func commit() {
        DeviceNameProvider.set(draft)
        // Refresh draft so an empty submission shows the auto default.
        draft = DeviceNameProvider.current()
    }
}
```

- [ ] **Step 2: Wire it into `SettingsView.accountSection`**

In `SettingsView.swift`, replace the `accountSection` (currently lines 34-48) with:

```swift
    private var accountSection: some View {
        Section(String(localized: "settings_account")) {
            LabeledContent(String(localized: "settings_username")) {
                Text(authManager.user?.username ?? "-")
            }
            LabeledContent(String(localized: "settings_role")) {
                Text(authManager.user?.role.capitalized ?? "-")
            }
            LabeledContent(String(localized: "settings_server")) {
                Text(authManager.serverUrl ?? "-")
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            DeviceNameRow()
        }
    }
```

- [ ] **Step 3: Add localization key**

Add to `Localizable.xcstrings`:

```json
"settings_device_name" : {
  "extractionState" : "manual",
  "localizations" : {
    "en"      : { "stringUnit" : { "state" : "translated", "value" : "Device Name" } },
    "zh-Hans" : { "stringUnit" : { "state" : "translated", "value" : "设备名称" } }
  }
}
```

- [ ] **Step 4: Build**

Run:
```bash
cd apps/ios && xcodegen generate && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee -destination 'generic/platform=iOS Simulator' -configuration Debug build -quiet
```
Expected: BUILD SUCCEEDED.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/Views/Settings/DeviceNameRow.swift apps/ios/ServerBee/Views/Settings/SettingsView.swift apps/ios/ServerBee/Localizable.xcstrings
git commit -m "feat(ios): add editable Device Name row in Settings"
```

---

## Task 7: Remove the in-app language Picker; document the decision

**Files:**
- Modify: `apps/ios/ServerBee/Views/Settings/AppearanceView.swift` (entire file)

- [ ] **Step 1: Replace the file** with a theme-only view plus an explanatory comment.

```swift
import SwiftUI

enum AppTheme: String, CaseIterable, Sendable {
    case system
    case light
    case dark

    var colorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }

    var localizedName: String {
        switch self {
        case .system: String(localized: "settings_theme_system")
        case .light: String(localized: "settings_theme_light")
        case .dark: String(localized: "settings_theme_dark")
        }
    }
}

// MARK: - Language story
//
// We deliberately do NOT expose an in-app language Picker. Reasons:
//
//   1. iOS 13+ ships a per-app language switcher at Settings → ServerBee →
//      Language, which is the platform-blessed path. Going through Settings
//      restarts the app cleanly and updates every bundle, including
//      system-provided UI such as alert buttons.
//   2. In-app switching via the `AppleLanguages` UserDefault requires an app
//      restart to take effect for `String(localized:)` lookups. Doing it
//      ourselves means either (a) lying to the user about the switch taking
//      effect immediately, or (b) shipping a custom restart UX. Neither is
//      worth the maintenance cost for a two-language app.
//   3. Our previous Picker wrote `@AppStorage("locale")` with zero downstream
//      effect — that was a UX bug.
//
// If we later need an in-app override (e.g., for accessibility), add it back
// with an explicit "restart required" affordance and route through
// `Bundle.main.preferredLocalizations` — not the system locale.

struct AppearanceView: View {
    @AppStorage("theme") private var theme: String = AppTheme.system.rawValue

    private var selectedTheme: AppTheme {
        AppTheme(rawValue: theme) ?? .system
    }

    var body: some View {
        List {
            Section(String(localized: "settings_theme")) {
                Picker(String(localized: "settings_theme"), selection: $theme) {
                    ForEach(AppTheme.allCases, id: \.rawValue) { option in
                        Text(option.localizedName).tag(option.rawValue)
                    }
                }
                .pickerStyle(.inline)
                .labelsHidden()
            }

            Section(String(localized: "settings_language")) {
                Text(String(localized: "settings_language_hint"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle(String(localized: "settings_appearance"))
        .preferredColorScheme(selectedTheme.colorScheme)
    }
}
```

- [ ] **Step 2: Add the hint localization key**

Add to `Localizable.xcstrings`:

```json
"settings_language_hint" : {
  "extractionState" : "manual",
  "localizations" : {
    "en"      : { "stringUnit" : { "state" : "translated", "value" : "Change the app language in iOS Settings → ServerBee → Language." } },
    "zh-Hans" : { "stringUnit" : { "state" : "translated", "value" : "在系统设置 → ServerBee → 语言中切换应用语言。" } }
  }
}
```

- [ ] **Step 3: Build**

Run:
```bash
cd apps/ios && xcodegen generate && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee -destination 'generic/platform=iOS Simulator' -configuration Debug build -quiet
```
Expected: BUILD SUCCEEDED.

- [ ] **Step 4: Verify bundle localization configuration**

Open `apps/ios/ServerBee/Info.plist` and add the following keys just before `</dict>` at the end of the dict (if not already present):

```xml
	<key>CFBundleLocalizations</key>
	<array>
		<string>en</string>
		<string>zh-Hans</string>
	</array>
	<key>CFBundleAllowMixedLocalizations</key>
	<true/>
```

- [ ] **Step 5: Mirror in `project.yml`**

Open `apps/ios/project.yml` and under `targets.ServerBee.settings.base` add (alphabetical order is fine):

```yaml
        INFOPLIST_KEY_CFBundleAllowMixedLocalizations: YES
```

So the `base:` block becomes:

```yaml
      base:
        INFOPLIST_FILE: ServerBee/Info.plist
        INFOPLIST_KEY_CFBundleAllowMixedLocalizations: YES
        PRODUCT_BUNDLE_IDENTIFIER: com.serverbee.mobile
        TARGETED_DEVICE_FAMILY: "1"
        SWIFT_STRICT_CONCURRENCY: complete
        CODE_SIGN_ENTITLEMENTS: ServerBee/ServerBee.entitlements
```

- [ ] **Step 6: Rebuild and verify**

Run:
```bash
cd apps/ios && xcodegen generate && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee -destination 'generic/platform=iOS Simulator' -configuration Debug build -quiet
```
Expected: BUILD SUCCEEDED.

- [ ] **Step 7: Manual verification**

Install the app on the simulator. Go to **Settings (iOS) → ServerBee → Language**. Confirm both **English** and **简体中文** are listed. Switch to 简体中文; the app should relaunch with Chinese strings. Document the result in the PR description.

- [ ] **Step 8: Commit**

```bash
git add apps/ios/ServerBee/Views/Settings/AppearanceView.swift apps/ios/ServerBee/Info.plist apps/ios/project.yml apps/ios/ServerBee/Localizable.xcstrings
git commit -m "refactor(ios): rely on iOS Settings for per-app language"
```

---

## Task 8: Move pair flow into `AuthViewModel`

**Files:**
- Modify: `apps/ios/ServerBee/ViewModels/AuthViewModel.swift` (add `PairError` + `pair(...)`)

- [ ] **Step 1: Add the pair error enum and method**

Append the following inside the `AuthViewModel` class, after `goBackToCredentials()`:

```swift
    enum PairError: LocalizedError, Equatable {
        case invalidServerUrl
        case invalidOrExpiredCode
        case endpointNotFound
        case rateLimited
        case validation
        case transport
        case http(Int)

        var errorDescription: String? {
            switch self {
            case .invalidServerUrl:
                String(localized: "Invalid server URL in QR code.")
            case .invalidOrExpiredCode:
                String(localized: "Invalid or expired QR code. Please try again.")
            case .endpointNotFound:
                String(localized: "Pairing endpoint not found. Check server version.")
            case .rateLimited:
                String(localized: "Too many attempts. Please try again later.")
            case .validation:
                String(localized: "Invalid pairing request. Please rescan the QR code.")
            case .transport:
                String(localized: "Connection failed. Please check the server URL.")
            case let .http(code):
                String(localized: "Pairing failed (HTTP \(code)).")
            }
        }
    }

    /// Redeems a pair code obtained from a QR scan and, on success, hydrates
    /// the `AuthManager`. Throws `PairError` so the View can surface a
    /// localized message; on `200` the manager is updated and the function
    /// returns the token response.
    @MainActor
    func pair(
        serverUrl rawUrl: String,
        code: String,
        authManager: AuthManager,
        session: URLSession = .shared
    ) async throws -> MobileTokenResponse {
        var serverUrl = rawUrl.trimmingCharacters(in: .whitespacesAndNewlines)
        if serverUrl.hasSuffix("/") {
            serverUrl = String(serverUrl.dropLast())
        }
        guard let url = URL(string: "\(serverUrl)/api/mobile/auth/pair") else {
            throw PairError.invalidServerUrl
        }

        let body: [String: String] = [
            "code": code,
            "installation_id": InstallationID.getOrCreate(),
            "device_name": DeviceNameProvider.current(),
        ]

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: body)

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch {
            throw PairError.transport
        }

        guard let http = response as? HTTPURLResponse else {
            throw PairError.transport
        }

        switch http.statusCode {
        case 200:
            let tokenResponse = try JSONDecoder.snakeCase.decode(
                ApiResponse<MobileTokenResponse>.self,
                from: data
            ).data
            authManager.setServerUrl(serverUrl)
            authManager.handleLoginResponse(tokenResponse)
            return tokenResponse
        case 400:
            throw PairError.invalidOrExpiredCode
        case 404:
            throw PairError.endpointNotFound
        case 422:
            // Backend uses 422 for pair only as a missing-field validation
            // error (see crates/server/src/router/api/mobile.rs:323-381).
            // There is NO 2FA branch on /api/mobile/auth/pair, so we never
            // route this into `step = .totp` like /auth/login does.
            throw PairError.validation
        case 429:
            throw PairError.rateLimited
        default:
            throw PairError.http(http.statusCode)
        }
    }
```

- [ ] **Step 2: Build**

Run:
```bash
cd apps/ios && xcodegen generate && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee -destination 'generic/platform=iOS Simulator' -configuration Debug build -quiet
```
Expected: BUILD SUCCEEDED.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/ViewModels/AuthViewModel.swift
git commit -m "refactor(ios): move pair flow into AuthViewModel"
```

---

## Task 9: Switch `LoginView` to call `viewModel.pair(...)`

**Files:**
- Modify: `apps/ios/ServerBee/Views/Auth/LoginView.swift` (remove inline `pair`, drive `viewModel.pair`)

- [ ] **Step 1: Delete the inline `pair(...)` block** (lines 104-161 inclusive) and replace the `.sheet` handler so the view holds only presentation state. The full updated file body (replace from the top of the struct down through the closing brace of `body`) is:

```swift
import SwiftUI
import UIKit

struct LoginView: View {
    @State private var viewModel = AuthViewModel()
    @State private var showQRScanner = false
    @State private var pairErrorMessage = ""
    @State private var isPairing = false
    @FocusState private var totpFocused: Bool
    @Environment(AuthManager.self) private var authManager

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(spacing: 24) {
                    VStack(spacing: 8) {
                        Image(systemName: "server.rack")
                            .font(.system(size: 60))
                            .foregroundStyle(Color.accentColor)
                        Text("ServerBee")
                            .font(.largeTitle.bold())
                    }
                    .padding(.top, 60)
                    .padding(.bottom, 20)

                    VStack(spacing: 16) {
                        if viewModel.step == .credentials {
                            credentialsFields
                        } else {
                            totpFields
                                .id("totp")
                        }

                        if !viewModel.errorMessage.isEmpty {
                            Text(viewModel.errorMessage)
                                .font(.subheadline)
                                .foregroundStyle(.red)
                                .multilineTextAlignment(.center)
                        }

                        if !pairErrorMessage.isEmpty {
                            Text(pairErrorMessage)
                                .font(.subheadline)
                                .foregroundStyle(.red)
                                .multilineTextAlignment(.center)
                        }

                        Button {
                            Task {
                                await viewModel.login(authManager: authManager)
                            }
                        } label: {
                            Group {
                                if viewModel.isLoading {
                                    ProgressView()
                                        .tint(.white)
                                } else {
                                    Text("Login")
                                        .fontWeight(.semibold)
                                }
                            }
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 14)
                        }
                        .background(Color.accentColor)
                        .foregroundStyle(.white)
                        .clipShape(RoundedRectangle(cornerRadius: 12))
                        .disabled(viewModel.isLoading || isPairing)

                        Button {
                            showQRScanner = true
                        } label: {
                            HStack(spacing: 8) {
                                Image(systemName: "qrcode.viewfinder")
                                Text("Scan QR Code")
                                    .fontWeight(.semibold)
                            }
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 14)
                        }
                        .background(Color(.systemGray5))
                        .foregroundStyle(Color.accentColor)
                        .clipShape(RoundedRectangle(cornerRadius: 12))
                        .disabled(isPairing)

                        if viewModel.step == .totp {
                            Button(String(localized: "Back")) {
                                viewModel.goBackToCredentials()
                            }
                            .foregroundStyle(.secondary)
                        }
                    }
                    .padding(.horizontal, 24)
                }
            }
            .scrollDismissesKeyboard(.interactively)
            .onChange(of: viewModel.step) { _, newStep in
                if newStep == .totp {
                    // Defer one runloop so the totp field exists before we
                    // scroll to it on iPhone SE-class devices where the
                    // keyboard otherwise covers the field.
                    DispatchQueue.main.async {
                        withAnimation { proxy.scrollTo("totp", anchor: .center) }
                        totpFocused = true
                    }
                }
            }
            .sheet(isPresented: $showQRScanner) {
                QRScannerView { serverUrl, code in
                    showQRScanner = false
                    Task { await runPair(serverUrl: serverUrl, code: code) }
                }
            }
        }
    }

    @MainActor
    private func runPair(serverUrl: String, code: String) async {
        isPairing = true
        pairErrorMessage = ""
        defer { isPairing = false }
        do {
            _ = try await viewModel.pair(serverUrl: serverUrl, code: code, authManager: authManager)
        } catch let error as AuthViewModel.PairError {
            pairErrorMessage = error.errorDescription ?? ""
        } catch {
            pairErrorMessage = String(localized: "Connection failed. Please check the server URL.")
        }
    }

    // MARK: - Subviews

    private var credentialsFields: some View {
        Group {
            LabeledField(label: String(localized: "Server URL")) {
                TextField("https://your-server.com", text: $viewModel.serverUrlInput)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            }

            LabeledField(label: String(localized: "Username")) {
                TextField(String(localized: "Username"), text: $viewModel.username)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            }

            LabeledField(label: String(localized: "Password")) {
                SecureField(String(localized: "Password"), text: $viewModel.password)
            }
        }
    }

    private var totpFields: some View {
        VStack(spacing: 12) {
            Text("Two-Factor Authentication")
                .font(.headline)
            Text("Enter the 6-digit code from your authenticator app")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

            TextField("000000", text: $viewModel.totpCode)
                .textFieldStyle(.roundedBorder)
                .keyboardType(.numberPad)
                .multilineTextAlignment(.center)
                .font(.title2.monospaced())
                .focused($totpFocused)
        }
    }
}

// MARK: - Labeled Field

private struct LabeledField<Content: View>: View {
    let label: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label)
                .font(.subheadline.weight(.medium))
            content
                .textFieldStyle(.roundedBorder)
        }
    }
}
```

- [ ] **Step 2: Build**

Run:
```bash
cd apps/ios && xcodegen generate && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee -destination 'generic/platform=iOS Simulator' -configuration Debug build -quiet
```
Expected: BUILD SUCCEEDED.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Views/Auth/LoginView.swift
git commit -m "refactor(ios): route LoginView pair through AuthViewModel and scroll TOTP into view"
```

---

## Task 10: Test pair happy path via `URLProtocolStub`

**Files:**
- Create: `apps/ios/ServerBeeTests/AuthViewModelPairTests.swift`

This task assumes Plan 1 already provides a `URLProtocolStub` helper in `ServerBeeTests`. If not, the test file below defines a local one (delete the local definition once Plan 1's shared helper lands).

- [ ] **Step 1: Write the failing happy-path test**

```swift
import XCTest
@testable import ServerBee

final class AuthViewModelPairTests: XCTestCase {
    private var session: URLSession!

    override func setUp() {
        super.setUp()
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [URLProtocolStub.self]
        session = URLSession(configuration: config)
        URLProtocolStub.reset()
    }

    override func tearDown() {
        URLProtocolStub.reset()
        session = nil
        super.tearDown()
    }

    @MainActor
    func test_pair_returnsToken_andHydratesAuthManager_on200() async throws {
        let payload = """
        {
          "data": {
            "access_token": "at",
            "access_expires_in_secs": 3600,
            "refresh_token": "rt",
            "refresh_expires_in_secs": 86400,
            "token_type": "Bearer",
            "user": { "id": "u1", "username": "alice", "role": "admin" }
          }
        }
        """.data(using: .utf8)!

        URLProtocolStub.stub(
            url: URL(string: "https://srv.example.com/api/mobile/auth/pair")!,
            statusCode: 200,
            data: payload
        )

        let viewModel = AuthViewModel()
        let authManager = AuthManager()

        let token = try await viewModel.pair(
            serverUrl: "https://srv.example.com/",
            code: "sb_pair_abc",
            authManager: authManager,
            session: session
        )

        XCTAssertEqual(token.accessToken, "at")
        XCTAssertEqual(authManager.serverUrl, "https://srv.example.com")
        XCTAssertEqual(authManager.user?.username, "alice")
    }
}

// MARK: - URLProtocolStub (delete once Plan 1's shared helper exists)

final class URLProtocolStub: URLProtocol {
    private struct Stub {
        let statusCode: Int
        let data: Data
    }
    private static var stubs: [URL: Stub] = [:]
    static func stub(url: URL, statusCode: Int, data: Data) {
        stubs[url] = Stub(statusCode: statusCode, data: data)
    }
    static func reset() { stubs = [:] }

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        guard let url = request.url, let stub = Self.stubs[url] else {
            client?.urlProtocol(self, didFailWithError: URLError(.fileDoesNotExist))
            return
        }
        let response = HTTPURLResponse(url: url, statusCode: stub.statusCode, httpVersion: "HTTP/1.1", headerFields: nil)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: stub.data)
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}
```

- [ ] **Step 2: Run** — expect failure if `AuthManager` cannot be constructed without arguments. If so, mirror the construction used in Plan 1/2 (e.g., `AuthManager(keychain: KeychainService.self)`) and re-run.

Run:
```bash
cd apps/ios && xcodegen generate && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' test -only-testing:ServerBeeTests/AuthViewModelPairTests/test_pair_returnsToken_andHydratesAuthManager_on200 -quiet
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBeeTests/AuthViewModelPairTests.swift
git commit -m "test(ios): cover AuthViewModel.pair happy path"
```

---

## Task 11: Test pair error mapping (400, 422, 429)

**Files:**
- Modify: `apps/ios/ServerBeeTests/AuthViewModelPairTests.swift`

- [ ] **Step 1: Append the failure-mapping tests**

```swift
extension AuthViewModelPairTests {
    @MainActor
    func test_pair_throwsInvalidOrExpiredCode_on400() async {
        URLProtocolStub.stub(
            url: URL(string: "https://srv.example.com/api/mobile/auth/pair")!,
            statusCode: 400,
            data: Data()
        )
        let viewModel = AuthViewModel()
        let authManager = AuthManager()
        do {
            _ = try await viewModel.pair(
                serverUrl: "https://srv.example.com",
                code: "x",
                authManager: authManager,
                session: session
            )
            XCTFail("Expected throw")
        } catch let error as AuthViewModel.PairError {
            XCTAssertEqual(error, .invalidOrExpiredCode)
        } catch {
            XCTFail("Unexpected error type: \(error)")
        }
    }

    @MainActor
    func test_pair_throwsValidation_on422_andDoesNotEnterTotpStep() async {
        URLProtocolStub.stub(
            url: URL(string: "https://srv.example.com/api/mobile/auth/pair")!,
            statusCode: 422,
            data: Data()
        )
        let viewModel = AuthViewModel()
        let authManager = AuthManager()
        do {
            _ = try await viewModel.pair(
                serverUrl: "https://srv.example.com",
                code: "x",
                authManager: authManager,
                session: session
            )
            XCTFail("Expected throw")
        } catch let error as AuthViewModel.PairError {
            XCTAssertEqual(error, .validation)
            XCTAssertEqual(viewModel.step, .credentials)
        } catch {
            XCTFail("Unexpected error type: \(error)")
        }
    }

    @MainActor
    func test_pair_throwsRateLimited_on429() async {
        URLProtocolStub.stub(
            url: URL(string: "https://srv.example.com/api/mobile/auth/pair")!,
            statusCode: 429,
            data: Data()
        )
        let viewModel = AuthViewModel()
        let authManager = AuthManager()
        do {
            _ = try await viewModel.pair(
                serverUrl: "https://srv.example.com",
                code: "x",
                authManager: authManager,
                session: session
            )
            XCTFail("Expected throw")
        } catch let error as AuthViewModel.PairError {
            XCTAssertEqual(error, .rateLimited)
        } catch {
            XCTFail("Unexpected error type: \(error)")
        }
    }

    @MainActor
    func test_pair_throwsInvalidServerUrl_onBadUrl() async {
        let viewModel = AuthViewModel()
        let authManager = AuthManager()
        do {
            _ = try await viewModel.pair(
                serverUrl: "::not a url::",
                code: "x",
                authManager: authManager,
                session: session
            )
            XCTFail("Expected throw")
        } catch let error as AuthViewModel.PairError {
            XCTAssertEqual(error, .invalidServerUrl)
        } catch {
            XCTFail("Unexpected error type: \(error)")
        }
    }
}
```

- [ ] **Step 2: Run the new tests**

Run:
```bash
cd apps/ios && xcodebuild -project ServerBee.xcodeproj -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' test -only-testing:ServerBeeTests/AuthViewModelPairTests -quiet
```
Expected: all 5 pair tests pass.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBeeTests/AuthViewModelPairTests.swift
git commit -m "test(ios): cover AuthViewModel.pair 400/422/429/url errors"
```

---

## Task 12: Manual keyboard-avoidance verification on iPhone SE

**Files:** none (manual verification).

- [ ] **Step 1: Boot iPhone SE simulator**

Run:
```bash
xcrun simctl boot "iPhone SE (3rd generation)" || true
open -a Simulator
```
Expected: simulator running.

- [ ] **Step 2: Trigger TOTP step**

Run the app on the booted device. Enter a valid server URL/username/password for a 2FA-enabled account so the server returns `422` and `viewModel.step` transitions to `.totp`.

Expected: the `000000` TOTP `TextField` is visible above the system keyboard and is focused automatically. Tapping the field should not cover it with the keyboard.

- [ ] **Step 3: Document the result in the PR description.** No commit.

---

## Self-Review

Spec coverage map:

| Issue | Task(s) |
|-------|---------|
| #5 QRScannerView session safety | 1, 2 |
| #6 Camera permission flow | 1, 2 |
| #8 Language Picker no-op | 7 |
| #12 `UIDevice.current.name` | 3, 4, 5, 6 |
| #23 pair flow duplicates ViewModel | 8, 9, 10, 11 |
| #33 keyboard covers TOTP | 9, 12 |

All issues mapped. No "TBD" / "TODO" / "fill in later" placeholders. Method names verified for consistency: `DeviceNameProvider.current(defaults:)`, `.set(_:defaults:)`, `.defaultName(defaults:)`; `AuthViewModel.pair(serverUrl:code:authManager:session:)`; `AuthViewModel.PairError` cases used in tests match definitions. `JSONDecoder.snakeCase` confirmed to exist in `Models/APIModels.swift:27`.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-20-ios-plan-4-login-camera-language.md`. Two execution options:

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task with two-stage review.
2. **Inline Execution** — use `superpowers:executing-plans` and batch with checkpoints.
