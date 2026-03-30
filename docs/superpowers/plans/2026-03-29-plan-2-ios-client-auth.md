# iOS Client Auth Integration Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix iOS client to work with the new server-side mobile auth API — centralize token refresh in AuthManager, auto-unwrap API responses, integrate WebSocket, and update all ViewModels.

**Architecture:** AuthManager becomes the single refresh owner via an internal `RefreshCoordinator` actor. APIClient delegates refresh to AuthManager. WebSocketClient gains a `tokenRefresher` closure for reconnect. ServersViewModel lifts to ContentView level via @Environment for WS integration.

**Tech Stack:** Swift 6, SwiftUI, @Observable, async/await, URLSession

**Spec:** `docs/superpowers/specs/2026-03-29-ios-mvp-design.md` Sections 1 (iOS Auth Models) + 3 + 5

---

### Task 1: AuthModels — Add `deviceName`, remove `MobileLogoutRequest`

**Files:**
- Modify: `apps/ios/ServerBee/Models/AuthModels.swift`

- [ ] **Step 1: Add `deviceName` to `MobileLoginRequest`**

Replace the current `MobileLoginRequest` struct with:

```swift
struct MobileLoginRequest: Codable, Sendable {
    let username: String
    let password: String
    let installationId: String
    let deviceName: String
    var totpCode: String?

    enum CodingKeys: String, CodingKey {
        case username
        case password
        case installationId = "installation_id"
        case deviceName = "device_name"
        case totpCode = "totp_code"
    }
}
```

- [ ] **Step 2: Remove `MobileLogoutRequest` typealias**

Delete the line `typealias MobileLogoutRequest = MobileRefreshRequest` (line 51).

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Models/AuthModels.swift
git commit -m "feat(ios): add deviceName to MobileLoginRequest, remove MobileLogoutRequest"
```

---

### Task 2: AuthManager — Centralize refresh with RefreshCoordinator actor

**Files:**
- Modify: `apps/ios/ServerBee/Services/AuthManager.swift`

- [ ] **Step 1: Add `RefreshCoordinator` private actor inside AuthManager**

Add at the bottom of `AuthManager.swift`, before the closing of the file:

```swift
/// Actor that serializes token refresh requests. Ensures only one refresh
/// request is in-flight at a time; concurrent callers wait for the same result.
private actor RefreshCoordinator {
    private var isRefreshing = false
    private var waiters: [CheckedContinuation<String, Error>] = []

    func refresh(using refreshFn: @Sendable () async throws -> String) async throws -> String {
        if isRefreshing {
            return try await withCheckedThrowingContinuation { continuation in
                waiters.append(continuation)
            }
        }

        isRefreshing = true
        do {
            let newToken = try await refreshFn()
            let pending = waiters
            waiters = []
            isRefreshing = false
            for waiter in pending {
                waiter.resume(returning: newToken)
            }
            return newToken
        } catch {
            let pending = waiters
            waiters = []
            isRefreshing = false
            for waiter in pending {
                waiter.resume(throwing: error)
            }
            throw error
        }
    }
}
```

- [ ] **Step 2: Add `refreshCoordinator` and public `refreshAccessToken()` to AuthManager**

Add a stored property to `AuthManager`:

```swift
    private let refreshCoordinator = RefreshCoordinator()
```

Add the public refresh method:

```swift
    /// Centralized token refresh. Both APIClient (on 401) and WebSocketClient
    /// (on reconnect) call this. Concurrent calls are coalesced by RefreshCoordinator.
    func refreshAccessToken() async throws -> String {
        try await refreshCoordinator.refresh { [self] in
            guard let refreshToken = KeychainService.loadString(for: KeychainService.refreshTokenKey) else {
                throw AuthError.refreshFailed
            }

            let response = try await refreshTokens(refreshToken: refreshToken)
            await handleLoginResponse(response)
            return response.accessToken
        }
    }
```

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Services/AuthManager.swift
git commit -m "feat(ios): centralize token refresh in AuthManager via RefreshCoordinator actor"
```

---

### Task 3: APIClient — Delegate refresh to AuthManager, auto-unwrap ApiResponse

**Files:**
- Modify: `apps/ios/ServerBee/Services/APIClient.swift`

- [ ] **Step 1: Change `request<T>` to auto-unwrap `ApiResponse<T>`**

The current `request<T>` decodes `T` directly. Change it to always decode `ApiResponse<T>` and return `.data`:

```swift
    private func request<T: Decodable>(
        _ path: String,
        method: String,
        body: (any Encodable & Sendable)? = nil
    ) async throws -> T {
        let (data, httpResponse) = try await performRequest(path, method: method, body: body)

        if httpResponse.statusCode == 401 {
            return try await handleUnauthorized(path: path, method: method, body: body)
        }

        guard (200...299).contains(httpResponse.statusCode) else {
            throw APIError.httpError(statusCode: httpResponse.statusCode, data: data)
        }

        do {
            let wrapper = try JSONDecoder.snakeCase.decode(ApiResponse<T>.self, from: data)
            return wrapper.data
        } catch {
            throw APIError.decodingError(error)
        }
    }
```

- [ ] **Step 2: Replace refresh logic with AuthManager delegation**

Remove `performTokenRefresh()`, `callRefreshEndpoint()`, `resumeWaiters()`, `isRefreshing`, and `refreshContinuations` from APIClient.

Replace `handleUnauthorized`:

```swift
    private func handleUnauthorized<T: Decodable>(
        path: String,
        method: String,
        body: (any Encodable & Sendable)?
    ) async throws -> T {
        do {
            _ = try await authManager.refreshAccessToken()
        } catch {
            await authManager.clearAuth()
            throw APIError.unauthorized
        }

        let (data, httpResponse) = try await performRequest(path, method: method, body: body)

        if httpResponse.statusCode == 401 {
            await authManager.clearAuth()
            throw APIError.unauthorized
        }

        guard (200...299).contains(httpResponse.statusCode) else {
            throw APIError.httpError(statusCode: httpResponse.statusCode, data: data)
        }

        do {
            let wrapper = try JSONDecoder.snakeCase.decode(ApiResponse<T>.self, from: data)
            return wrapper.data
        } catch {
            throw APIError.decodingError(error)
        }
    }
```

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Services/APIClient.swift
git commit -m "feat(ios): APIClient auto-unwraps ApiResponse, delegates refresh to AuthManager"
```

---

### Task 4: ViewModels — Remove manual ApiResponse unwrapping

**Files:**
- Modify: `apps/ios/ServerBee/ViewModels/ServersViewModel.swift`
- Modify: `apps/ios/ServerBee/ViewModels/ServerDetailViewModel.swift`
- Modify: `apps/ios/ServerBee/ViewModels/AlertsViewModel.swift`
- Modify: `apps/ios/ServerBee/ViewModels/AlertDetailViewModel.swift`

All these currently do `let response: ApiResponse<X> = try await apiClient.get(...); foo = response.data`. Change to `let foo: X = try await apiClient.get(...)`.

- [ ] **Step 1: Fix ServersViewModel.fetchServers**

```swift
    func fetchServers(apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            servers = try await apiClient.get("/api/servers")
        } catch {
            print("[Servers] Fetch failed: \(error)")
        }
    }
```

- [ ] **Step 2: Fix ServerDetailViewModel**

```swift
    func fetchDetail(serverId: String, apiClient: APIClient) async {
        guard server == nil else { return }
        do {
            server = try await apiClient.get("/api/servers/\(serverId)")
        } catch {
            print("[ServerDetail] Fetch failed: \(error)")
        }
    }

    func fetchRecords(serverId: String, range: String, apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            records = try await apiClient.get("/api/servers/\(serverId)/records?range=\(range)")
        } catch {
            print("[ServerDetail] Records fetch failed: \(error)")
        }
    }
```

- [ ] **Step 3: Fix AlertsViewModel**

```swift
    func fetchEvents(limit: Int = 50, apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            events = try await apiClient.get("/api/alert-events?limit=\(limit)")
        } catch {
            print("[Alerts] Fetch failed: \(error)")
        }
    }
```

- [ ] **Step 4: Fix AlertDetailViewModel — also change URL path**

```swift
    func fetchDetail(alertKey: String, apiClient: APIClient) async {
        isLoading = true
        defer { isLoading = false }
        do {
            detail = try await apiClient.get("/api/alert-events/\(alertKey)")
        } catch {
            errorMessage = String(localized: "Alert not found")
        }
    }
```

Note: URL changed from `/api/mobile/alerts/` to `/api/alert-events/` to match the server endpoint.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/ViewModels/
git commit -m "feat(ios): update all ViewModels to use auto-unwrapped API responses"
```

---

### Task 5: AuthViewModel — Add device_name to login request

**Files:**
- Modify: `apps/ios/ServerBee/ViewModels/AuthViewModel.swift`

- [ ] **Step 1: Add device_name to login request construction**

In the `login()` method, update the `MobileLoginRequest` construction (around line 34):

```swift
        let loginRequest = MobileLoginRequest(
            username: username,
            password: password,
            installationId: installationId,
            deviceName: UIDevice.current.name,
            totpCode: step == .totp ? totpCode : nil
        )
```

Add `import UIKit` at the top of the file if not already present.

- [ ] **Step 2: Commit**

```bash
git add apps/ios/ServerBee/ViewModels/AuthViewModel.swift
git commit -m "feat(ios): send device_name in mobile login request"
```

---

### Task 6: SettingsViewModel — Fix logout to no-body POST

**Files:**
- Modify: `apps/ios/ServerBee/ViewModels/SettingsViewModel.swift`

- [ ] **Step 1: Simplify logout to bearer-only POST**

Replace the `logout` method:

```swift
    func logout(authManager: AuthManager, apiClient: APIClient) async {
        isLoggingOut = true
        defer { isLoggingOut = false }

        // Best effort: POST logout to server (Bearer token provides identity)
        try? await apiClient.postVoid("/api/mobile/auth/logout")

        await authManager.clearAuth()
    }
```

No body needed — the server identifies the device via the Bearer token.

- [ ] **Step 2: Remove unused imports if any**

The `KeychainService` and `InstallationID` imports are no longer needed in this file.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/ViewModels/SettingsViewModel.swift
git commit -m "feat(ios): simplify logout to body-less Bearer POST"
```

---

### Task 7: WebSocketClient — Add tokenRefresher for reconnect

**Files:**
- Modify: `apps/ios/ServerBee/Services/WebSocketClient.swift`

- [ ] **Step 1: Add `tokenRefresher` property**

Add to the public properties section:

```swift
    /// Called before reconnect to obtain a fresh access token.
    var tokenRefresher: (@Sendable () async -> String?)?
```

- [ ] **Step 2: Update `scheduleReconnect` to refresh token before reconnecting**

Replace the `scheduleReconnect` method:

```swift
    private func scheduleReconnect() async {
        guard !intentionallyClosed else { return }

        let jitter = 1.0 + (Double.random(in: -1 ... 1) * jitterFactor)
        let delay = min(reconnectDelay * jitter, maxReconnectDelay)

        try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))

        guard !intentionallyClosed, !Task.isCancelled else { return }

        reconnectDelay = min(reconnectDelay * 2, maxReconnectDelay)

        // Refresh token before reconnecting
        if let refresher = tokenRefresher {
            if let newToken = await refresher() {
                currentAccessToken = newToken
            } else {
                // Refresh failed — stop reconnecting
                await MainActor.run { [weak self] in
                    self?.connectionState = .disconnected
                }
                return
            }
        }

        await MainActor.run { [weak self] in
            self?.establishConnection()
        }
    }
```

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Services/WebSocketClient.swift
git commit -m "feat(ios): WebSocketClient refreshes token before reconnect"
```

---

### Task 8: ContentView — Integrate WebSocket + lift ServersViewModel

**Files:**
- Modify: `apps/ios/ServerBee/ContentView.swift`
- Modify: `apps/ios/ServerBee/Views/Servers/ServersListView.swift`

- [ ] **Step 1: Rewrite ContentView**

Replace the entire `ContentView` body:

```swift
import SwiftUI

struct ContentView: View {
    @Environment(AuthManager.self) private var authManager
    @State private var apiClient: APIClient?
    @State private var serversViewModel = ServersViewModel()
    @State private var wsClient = WebSocketClient()

    var body: some View {
        TabView {
            NavigationStack {
                ServersListView()
            }
            .tabItem {
                Label("Servers", systemImage: "server.rack")
            }

            NavigationStack {
                AlertsListView()
            }
            .tabItem {
                Label("Alerts", systemImage: "bell.badge")
            }

            SettingsView()
                .tabItem {
                    Label("Settings", systemImage: "gearshape")
                }
        }
        .environment(\.apiClient, apiClient)
        .environment(serversViewModel)
        .task {
            let client = APIClient(authManager: authManager)
            apiClient = client

            // Configure WS token refresher
            wsClient.tokenRefresher = { [weak authManager] in
                guard let authManager else { return nil }
                return try? await authManager.refreshAccessToken()
            }

            // Connect WebSocket
            wsClient.onMessage = { [weak serversViewModel] message in
                serversViewModel?.handleWSMessage(message)
            }
            if let serverUrl = authManager.serverUrl,
               let token = authManager.getAccessToken() {
                wsClient.connect(serverUrl: serverUrl, accessToken: token)
            }
        }
        .onDisappear {
            wsClient.close()
        }
    }
}

#Preview {
    ContentView()
        .environment(AuthManager())
        .environment(AlertsViewModel())
}
```

- [ ] **Step 2: Update ServersListView to use @Environment for ServersViewModel**

Replace the top of `ServersListView`:

```swift
struct ServersListView: View {
    @Environment(AuthManager.self) private var authManager
    @Environment(ServersViewModel.self) private var viewModel
    @Environment(\.apiClient) private var apiClient
```

Remove the `@State private var viewModel = ServersViewModel()` and `@State private var apiClient: APIClient?` lines.

Update the `.task` modifier — remove APIClient creation since it comes from environment:

```swift
        .task {
            if viewModel.servers.isEmpty, let apiClient {
                await viewModel.fetchServers(apiClient: apiClient)
            }
        }
```

Update `.refreshable`:

```swift
        .refreshable {
            if let apiClient {
                await viewModel.refresh(apiClient: apiClient)
            }
        }
```

Update the Preview:

```swift
#Preview {
    NavigationStack {
        ServersListView()
    }
    .environment(AuthManager())
    .environment(ServersViewModel())
}
```

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/ContentView.swift apps/ios/ServerBee/Views/Servers/ServersListView.swift
git commit -m "feat(ios): integrate WebSocket in ContentView, lift ServersViewModel to environment"
```
