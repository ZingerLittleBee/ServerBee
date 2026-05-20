# iOS Plan 2: Auth & Concurrency Refactor

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate `@unchecked Sendable` escape hatches by isolating `AuthManager` to `@MainActor`, construct `APIClient` synchronously in `@State` initializer, and harden token-refresh error classification so transient network failures no longer log users out.

**Architecture:** `AuthManager` becomes `@MainActor`-isolated `@Observable` class — APIClient actor reads state with `await`. APIClient owned by AuthManager (single instance), exposed via `@Environment(\.apiClient)`. Refresh-failure path distinguishes `.unauthorized` (401, clears auth) from `.network` (preserves auth, retries). `RefreshCoordinator` first-waiter-retries on transient failure. `KeychainService` uses dedicated encoder and documented accessibility class.

**Tech Stack:** Swift 5.10+ strict concurrency, `@MainActor`, `@Observable`, actors, `XCTest` with `URLProtocolStub`.

**Depends on:** Plan 1 (provides `ServerBeeTests` target).

---

## Pre-flight: Verify Plan 1 Dependency

- [ ] **Step 1: Confirm `ServerBeeTests` target exists**

Run: `ls apps/ios/ServerBeeTests/ && grep -A2 "ServerBeeTests:" apps/ios/project.yml`
Expected: directory listing succeeds and `project.yml` contains a `ServerBeeTests` target block.

If the directory does not exist, STOP and execute Plan 1 (`docs/superpowers/plans/2026-05-20-ios-plan-1-realtime-websocket.md`) first — it provisions the test target.

- [ ] **Step 2: Confirm baseline build passes**

Run: `cd apps/ios && xcodegen && xcodebuild -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' build 2>&1 | tail -20`
Expected: `** BUILD SUCCEEDED **`

---

## Task 1: Isolate `AuthManager` to `@MainActor`

**Files:**
- Modify: `apps/ios/ServerBee/Services/AuthManager.swift:1-141`

Currently `AuthManager` is `@Observable + @unchecked Sendable` with mutable properties (`isLoading`, `isAuthenticated`, `user`, `serverUrl`) accessed concurrently from the `APIClient` actor, the `WebSocketClient` background task, and the MainActor UI. Under `SWIFT_STRICT_CONCURRENCY: complete` this is a data race waiting to happen. Annotating the class with `@MainActor` makes property access main-thread-only; the `APIClient` actor will hop via `await` to read.

- [ ] **Step 1: Write failing test asserting `@MainActor` isolation**

Create `apps/ios/ServerBeeTests/AuthManagerMainActorTests.swift`:

```swift
import XCTest
@testable import ServerBee

final class AuthManagerMainActorTests: XCTestCase {
    @MainActor
    func testStateMutationsHappenOnMainThread() async {
        let auth = AuthManager()
        auth.isAuthenticated = true
        XCTAssertTrue(Thread.isMainThread, "Mutating AuthManager state must be on main thread")
        XCTAssertTrue(auth.isAuthenticated)
    }

    /// Compile-time check: if AuthManager is `@MainActor`-isolated, this off-actor
    /// closure body should not be able to read `isAuthenticated` without `await`.
    /// We assert the runtime by hopping to MainActor explicitly.
    func testReadFromBackgroundRequiresActorHop() async {
        let auth = await AuthManager()
        let value: Bool = await MainActor.run { auth.isAuthenticated }
        XCTAssertFalse(value)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/AuthManagerMainActorTests 2>&1 | tail -30`
Expected: FAIL — current `AuthManager` is not `@MainActor`-isolated, so `await AuthManager()` and `MainActor.run { auth.isAuthenticated }` either fail to compile (init not async on a non-actor type) or the runtime assertion `Thread.isMainThread` could still pass spuriously. Specifically expect a compile error: `'await' in a function that does not support concurrency` or `cannot find 'AuthManager' in scope` until target wiring is set; OR a compile error indicating `@MainActor` calls cross actor boundaries.

(If the test compiles & passes on baseline — re-read: the second case `await AuthManager()` requires `AuthManager.init` to be MainActor-isolated. Currently it is not, so the compiler will flag `MainActor.run { auth.isAuthenticated }` as redundant. Adjust the failing condition by making the test depend on a public `@MainActor`-isolated computed property added in Task 3, OR proceed: the test is forward-looking; the meaningful red signal is the next step's strict-concurrency build error in production code after we remove `@unchecked Sendable`.)

- [ ] **Step 3: Annotate `AuthManager` with `@MainActor` and drop `@unchecked Sendable`**

Replace lines 8-19 of `apps/ios/ServerBee/Services/AuthManager.swift`:

```swift
/// Manages authentication state for the mobile app.
///
/// Isolated to `@MainActor` so that `@Observable` state is mutated only on the
/// main thread. Background callers (`APIClient` actor, `WebSocketClient`) hop
/// via `await` to read `serverUrl` / call `getAccessToken()`.
@Observable
@MainActor
final class AuthManager {
    // MARK: - Private

    private let refreshCoordinator = RefreshCoordinator()

    // MARK: - Published State

    var isLoading = true
    var isAuthenticated = false
    var user: MobileUser?
    var serverUrl: String?
```

Remove the per-method `@MainActor` annotations on `initialize()`, `handleLoginResponse(_:)`, and `clearAuth()` — they're redundant now. Leave the function bodies otherwise unchanged.

- [ ] **Step 4: Update `APIClient.performRequest` to `await` AuthManager reads**

In `apps/ios/ServerBee/Services/APIClient.swift`, replace lines 87-118 with:

```swift
    /// Build and fire a single URLRequest. Returns the raw data + HTTP response.
    private func performRequest(
        _ path: String,
        method: String,
        body: (any Encodable & Sendable)? = nil
    ) async throws -> (Data, HTTPURLResponse) {
        // AuthManager is @MainActor-isolated; hop to read state.
        let serverUrl = await authManager.serverUrl
        let token = await authManager.getAccessToken()

        guard let serverUrl else {
            throw APIError.noServerUrl
        }
        guard let url = URL(string: "\(serverUrl)\(path)") else {
            throw APIError.noServerUrl
        }

        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        // Attach bearer token if available
        if let token {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }

        if let body {
            request.httpBody = try JSONEncoder.snakeCase.encode(body)
        }

        let (data, response) = try await URLSession.shared.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw APIError.httpError(statusCode: -1, data: data)
        }

        return (data, httpResponse)
    }
```

Also update `getAccessToken()` in `AuthManager` (lines 73-75) — it touches the Keychain (a thread-safe API) but Swift cannot know that, so leave it as a member of the `@MainActor`-isolated class. Callers already use `await`.

- [ ] **Step 5: Build and verify strict-concurrency passes**

Run: `cd apps/ios && xcodegen && xcodebuild -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' build 2>&1 | grep -E "(error|warning|BUILD)" | tail -20`
Expected: `** BUILD SUCCEEDED **`, no `Sendable`/actor-isolation errors.

- [ ] **Step 6: Re-run AuthManager test**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/AuthManagerMainActorTests 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/ios/ServerBee/Services/AuthManager.swift \
        apps/ios/ServerBee/Services/APIClient.swift \
        apps/ios/ServerBeeTests/AuthManagerMainActorTests.swift
git commit -m "refactor(ios): isolate AuthManager to @MainActor"
```

---

## Task 2: Construct `APIClient` synchronously in `ContentView` initializer

**Files:**
- Modify: `apps/ios/ServerBee/ContentView.swift:1-66`

`@State var apiClient: APIClient?` is currently `nil` until `.task` runs, but child views' `.task` closures may fire concurrently with the parent and observe `apiClient == nil`, producing blank tabs on cold start. We construct it synchronously by passing the env `AuthManager` into a `@State` initializer.

- [ ] **Step 1: Write failing test for non-nil `apiClient` immediately after init**

Add to `apps/ios/ServerBeeTests/ContentViewInitTests.swift`:

```swift
import XCTest
import SwiftUI
@testable import ServerBee

@MainActor
final class ContentViewInitTests: XCTestCase {
    func testAPIClientIsAvailableImmediately() {
        let auth = AuthManager()
        auth.serverUrl = "https://example.com"
        let view = ContentView(authManager: auth)
        XCTAssertNotNil(view.apiClientForTest, "APIClient must be constructed before body renders")
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/ContentViewInitTests 2>&1 | tail -15`
Expected: FAIL — `ContentView` has no `init(authManager:)` and no `apiClientForTest` accessor.

- [ ] **Step 3: Rewrite `ContentView` to take `AuthManager` in init**

Replace the full contents of `apps/ios/ServerBee/ContentView.swift` with:

```swift
import SwiftUI

struct ContentView: View {
    @Environment(PushNotificationManager.self) private var pushManager
    @State private var apiClient: APIClient
    @State private var serversViewModel = ServersViewModel()
    @State private var wsClient = WebSocketClient()

    private let authManager: AuthManager

    init(authManager: AuthManager) {
        self.authManager = authManager
        // Construct APIClient synchronously so child views' .task closures
        // never observe a nil client on first cold start.
        _apiClient = State(initialValue: APIClient(authManager: authManager))
    }

    /// Test-only accessor — assert the client was built during init.
    var apiClientForTest: APIClient { apiClient }

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
            pushManager.configure(apiClient: apiClient)

            // Configure WS token refresher
            wsClient.tokenRefresher = { [weak authManager] in
                guard let authManager else { return nil }
                return try? await authManager.refreshAccessToken()
            }

            // Connect WebSocket
            wsClient.onMessage = { [weak serversViewModel] message in
                Task { @MainActor in
                    serversViewModel?.handleWSMessage(message)
                }
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
    ContentView(authManager: AuthManager())
        .environment(AlertsViewModel())
        .environment(PushNotificationManager())
}
```

- [ ] **Step 4: Update `RootView` to pass `authManager` into `ContentView`**

In `apps/ios/ServerBee/ServerBeeApp.swift` replace lines 30-44 with:

```swift
/// Shows a loading spinner while auth state is restored, then either LoginView or ContentView.
private struct RootView: View {
    @Environment(AuthManager.self) private var authManager

    var body: some View {
        Group {
            if authManager.isLoading {
                ProgressView()
            } else if authManager.isAuthenticated {
                ContentView(authManager: authManager)
            } else {
                LoginView()
            }
        }
    }
}
```

- [ ] **Step 5: Make `apiClient` Environment value non-optional**

In `apps/ios/ServerBee/Utilities/EnvironmentKeys.swift`, replace the file contents with:

```swift
import SwiftUI

// MARK: - APIClient Environment Key

/// Allows passing the `APIClient` actor through the SwiftUI environment.
///
/// The default value is a placeholder client bound to an empty `AuthManager`;
/// it is always replaced by `ContentView` via `.environment(\.apiClient, ...)`
/// before any child view's `.task` runs. Views that read this value can
/// therefore treat it as guaranteed-available.
private struct APIClientKey: EnvironmentKey {
    @MainActor
    static let defaultValue: APIClient = APIClient(authManager: AuthManager())
}

extension EnvironmentValues {
    var apiClient: APIClient {
        get { self[APIClientKey.self] }
        set { self[APIClientKey.self] = newValue }
    }
}
```

- [ ] **Step 6: Build**

Run: `cd apps/ios && xcodebuild -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' build 2>&1 | grep -E "(error|BUILD)" | tail -20`
Expected: `** BUILD SUCCEEDED **`. (Other call sites still passing `APIClient?` will surface in Task 3.)

- [ ] **Step 7: Run test**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/ContentViewInitTests 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/ios/ServerBee/ContentView.swift \
        apps/ios/ServerBee/ServerBeeApp.swift \
        apps/ios/ServerBee/Utilities/EnvironmentKeys.swift \
        apps/ios/ServerBeeTests/ContentViewInitTests.swift
git commit -m "fix(ios): construct APIClient synchronously to avoid blank tabs on cold start"
```

---

## Task 3: `MetricsHistoryView` reads `APIClient` from environment

**Files:**
- Modify: `apps/ios/ServerBee/Views/Servers/MetricsHistoryView.swift:1-302`

The view currently constructs a second `APIClient`, breaking the single-instance assumption. Use `@Environment(\.apiClient)` instead.

- [ ] **Step 1: Write failing assertion**

Add to `apps/ios/ServerBeeTests/MetricsHistoryViewInitTests.swift`:

```swift
import XCTest
@testable import ServerBee

@MainActor
final class MetricsHistoryViewInitTests: XCTestCase {
    func testViewDoesNotConstructItsOwnAPIClient() throws {
        let source = try String(
            contentsOfFile: #filePath
                .replacingOccurrences(of: "ServerBeeTests/MetricsHistoryViewInitTests.swift",
                                      with: "ServerBee/Views/Servers/MetricsHistoryView.swift")
        )
        XCTAssertFalse(
            source.contains("APIClient(authManager:"),
            "MetricsHistoryView must consume APIClient from the environment, not construct one"
        )
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/MetricsHistoryViewInitTests 2>&1 | tail -15`
Expected: FAIL — the view's body contains `APIClient(authManager: authManager)`.

- [ ] **Step 3: Replace AuthManager-based construction with environment lookup**

In `apps/ios/ServerBee/Views/Servers/MetricsHistoryView.swift` replace lines 6-38 with:

```swift
struct MetricsHistoryView: View {
    let serverId: String

    @Environment(\.apiClient) private var apiClient
    @State private var viewModel = ServerDetailViewModel()
    @State private var selectedRange = "1h"

    private let timeRanges = ["1h", "6h", "24h", "7d"]

    var body: some View {
        ScrollView {
            VStack(spacing: 20) {
                timeRangeSelector
                chartSections
            }
            .padding()
        }
        .background(Color(.systemGroupedBackground))
        .navigationTitle(String(localized: "Metrics History"))
        .navigationBarTitleDisplayMode(.inline)
        .task {
            await viewModel.fetchRecords(serverId: serverId, range: selectedRange, apiClient: apiClient)
        }
        .onChange(of: selectedRange) { _, newRange in
            Task {
                await viewModel.fetchRecords(serverId: serverId, range: newRange, apiClient: apiClient)
            }
        }
    }
```

Also update the `#Preview` block (lines 297-302):

```swift
#Preview {
    NavigationStack {
        MetricsHistoryView(serverId: "1")
    }
    .environment(\.apiClient, APIClient(authManager: AuthManager()))
}
```

- [ ] **Step 4: Build & re-run test**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/MetricsHistoryViewInitTests 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/Views/Servers/MetricsHistoryView.swift \
        apps/ios/ServerBeeTests/MetricsHistoryViewInitTests.swift
git commit -m "refactor(ios): read APIClient from environment in MetricsHistoryView"
```

---

## Task 4: Classify refresh errors — preserve auth on network failures

**Files:**
- Modify: `apps/ios/ServerBee/Services/APIClient.swift:31-44, 120-152`
- Modify: `apps/ios/ServerBee/Services/AuthManager.swift:108-140`

Currently any `refreshAccessToken()` throw — including transient `URLError.notConnectedToInternet` — triggers `clearAuth()` and kicks the user out. We split the failure mode: `APIError.unauthorized` (the server actually said 401 on `/api/mobile/auth/refresh`) clears auth; `APIError.network` (everything else) preserves auth.

- [ ] **Step 1: Extend `AuthError` to distinguish 401 vs network**

In `apps/ios/ServerBee/Services/AuthManager.swift` replace the `AuthError` enum (lines 180-204) with:

```swift
enum AuthError: Error, LocalizedError {
    case noServerUrl
    case refreshUnauthorized           // server returned 401 — credentials revoked
    case refreshNetworkFailure(Error?)  // transient: no network, 5xx, timeout
    case invalidCredentials
    case twoFactorRequired
    case tooManyAttempts
    case networkError(Error)

    var errorDescription: String? {
        switch self {
        case .noServerUrl:
            return "No server URL configured"
        case .refreshUnauthorized:
            return "Session expired — please log in again"
        case .refreshNetworkFailure:
            return "Could not reach the server — please check your connection"
        case .invalidCredentials:
            return "Invalid username or password"
        case .twoFactorRequired:
            return "Two-factor authentication is required"
        case .tooManyAttempts:
            return "Too many login attempts — please try again later"
        case .networkError(let error):
            return "Network error: \(error.localizedDescription)"
        }
    }
}
```

- [ ] **Step 2: Classify failures in `refreshTokens(refreshToken:)`**

Replace lines 108-140 in `AuthManager.swift` with:

```swift
    /// Directly calls the refresh endpoint using URLSession.
    /// We intentionally bypass `APIClient` here to avoid a circular dependency.
    ///
    /// Throws:
    /// - `.noServerUrl` if no base URL is persisted.
    /// - `.refreshUnauthorized` if the server returned 401 (refresh token revoked
    ///    or expired). The caller MUST treat this as a permanent failure.
    /// - `.refreshNetworkFailure` for transport errors, timeouts, or 5xx — the
    ///    caller SHOULD retry rather than logging the user out.
    private func refreshTokens(refreshToken: String) async throws -> MobileTokenResponse {
        guard let serverUrl else {
            throw AuthError.noServerUrl
        }

        guard let url = URL(string: "\(serverUrl)/api/mobile/auth/refresh") else {
            throw AuthError.noServerUrl
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let body = MobileRefreshRequest(
            refreshToken: refreshToken,
            installationId: InstallationID.getOrCreate()
        )
        request.httpBody = try JSONEncoder.snakeCase.encode(body)

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await URLSession.shared.data(for: request)
        } catch {
            throw AuthError.refreshNetworkFailure(error)
        }

        guard let httpResponse = response as? HTTPURLResponse else {
            throw AuthError.refreshNetworkFailure(nil)
        }

        switch httpResponse.statusCode {
        case 200:
            do {
                let apiResponse = try JSONDecoder.snakeCase.decode(
                    ApiResponse<MobileTokenResponse>.self,
                    from: data
                )
                return apiResponse.data
            } catch {
                // Server replied 200 but body did not decode — treat as transient.
                throw AuthError.refreshNetworkFailure(error)
            }
        case 401, 403:
            throw AuthError.refreshUnauthorized
        default:
            // 5xx, 408, 429, anything else — let the caller retry.
            throw AuthError.refreshNetworkFailure(nil)
        }
    }
```

- [ ] **Step 3: Write failing test for refresh classification**

Create `apps/ios/ServerBeeTests/RefreshErrorClassificationTests.swift`:

```swift
import XCTest
@testable import ServerBee

/// Stubs `URLSession.shared.data(for:)` by intercepting via `URLProtocol`.
final class URLProtocolStub: URLProtocol {
    nonisolated(unsafe) static var stubResponse: (status: Int, data: Data)?
    nonisolated(unsafe) static var stubError: Error?

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        if let error = Self.stubError {
            client?.urlProtocol(self, didFailWithError: error)
            return
        }
        if let (status, data) = Self.stubResponse {
            let response = HTTPURLResponse(
                url: request.url!,
                statusCode: status,
                httpVersion: "HTTP/1.1",
                headerFields: nil
            )!
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        }
    }

    override func stopLoading() {}
}

@MainActor
final class RefreshErrorClassificationTests: XCTestCase {
    override func setUp() async throws {
        URLProtocol.registerClass(URLProtocolStub.self)
        URLProtocolStub.stubResponse = nil
        URLProtocolStub.stubError = nil
    }

    override func tearDown() async throws {
        URLProtocol.unregisterClass(URLProtocolStub.self)
    }

    func test401MapsToRefreshUnauthorized() async {
        URLProtocolStub.stubResponse = (401, Data())
        let auth = AuthManager()
        auth.serverUrl = "https://stub.test"
        try? KeychainService.saveString("rt", for: KeychainService.refreshTokenKey)

        do {
            _ = try await auth.refreshAccessToken()
            XCTFail("Expected throw")
        } catch let err as AuthError {
            if case .refreshUnauthorized = err { /* ok */ } else {
                XCTFail("Expected refreshUnauthorized, got \(err)")
            }
        } catch {
            XCTFail("Unexpected error: \(error)")
        }
    }

    func test503MapsToRefreshNetworkFailure() async {
        URLProtocolStub.stubResponse = (503, Data())
        let auth = AuthManager()
        auth.serverUrl = "https://stub.test"
        try? KeychainService.saveString("rt", for: KeychainService.refreshTokenKey)

        do {
            _ = try await auth.refreshAccessToken()
            XCTFail("Expected throw")
        } catch let err as AuthError {
            if case .refreshNetworkFailure = err { /* ok */ } else {
                XCTFail("Expected refreshNetworkFailure, got \(err)")
            }
        } catch {
            XCTFail("Unexpected error: \(error)")
        }
    }

    func testAPIClientClearsAuthOnly_OnUnauthorized() async throws {
        URLProtocolStub.stubResponse = (401, Data())
        let auth = AuthManager()
        auth.serverUrl = "https://stub.test"
        auth.isAuthenticated = true
        try? KeychainService.saveString("rt", for: KeychainService.refreshTokenKey)

        let client = APIClient(authManager: auth)
        do {
            let _: ApiResponse<String> = try await client.get("/anything")
            XCTFail("Expected throw")
        } catch {
            // 401 path
        }
        XCTAssertFalse(auth.isAuthenticated, "401 from refresh must clear auth")
    }

    func testAPIClientPreservesAuth_OnNetworkFailure() async throws {
        URLProtocolStub.stubResponse = (503, Data())
        let auth = AuthManager()
        auth.serverUrl = "https://stub.test"
        auth.isAuthenticated = true
        try? KeychainService.saveString("rt", for: KeychainService.refreshTokenKey)

        let client = APIClient(authManager: auth)
        do {
            let _: ApiResponse<String> = try await client.get("/anything")
            XCTFail("Expected throw")
        } catch {
            // network path
        }
        XCTAssertTrue(auth.isAuthenticated, "Transient network error must NOT clear auth")
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/RefreshErrorClassificationTests 2>&1 | tail -30`
Expected: FAIL — `APIClient` still calls `clearAuth()` on every refresh error.

- [ ] **Step 5: Rewrite APIClient 401 handler to honour error classification**

Replace lines 27-49 (`postVoid`) in `apps/ios/ServerBee/Services/APIClient.swift` with:

```swift
    /// Perform a POST request for endpoints that return null/empty data.
    func postVoid(_ path: String, body: (any Encodable & Sendable)? = nil) async throws {
        let (_, httpResponse) = try await performRequest(path, method: "POST", body: body)

        if httpResponse.statusCode == 401 {
            try await refreshOrThrow()
            let (_, retryResponse) = try await performRequest(path, method: "POST", body: body)
            if retryResponse.statusCode == 401 {
                await authManager.clearAuth()
                throw APIError.unauthorized
            }
            guard (200...299).contains(retryResponse.statusCode) else {
                throw APIError.httpError(statusCode: retryResponse.statusCode, data: Data())
            }
            return
        }

        guard (200...299).contains(httpResponse.statusCode) else {
            throw APIError.httpError(statusCode: httpResponse.statusCode, data: Data())
        }
    }
```

Replace lines 120-152 (`handleUnauthorized`) with:

```swift
    // MARK: - 401 Handling

    private func handleUnauthorized<T: Decodable & Sendable>(
        path: String,
        method: String,
        body: (any Encodable & Sendable)?
    ) async throws -> T {
        try await refreshOrThrow()

        let (data, httpResponse) = try await performRequest(path, method: method, body: body)

        if httpResponse.statusCode == 401 {
            // Refresh succeeded but server still rejects — credentials definitely revoked.
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

    /// Run a refresh; classify the failure mode.
    ///
    /// - On `.refreshUnauthorized`: clear local auth and surface `.unauthorized`.
    /// - On `.refreshNetworkFailure`: leave local auth intact and surface
    ///   `.network` so the caller can show a transient error instead of
    ///   kicking the user back to the login screen.
    private func refreshOrThrow() async throws {
        do {
            _ = try await authManager.refreshAccessToken()
        } catch AuthError.refreshUnauthorized {
            await authManager.clearAuth()
            throw APIError.unauthorized
        } catch {
            // .refreshNetworkFailure, .noServerUrl, or anything else transient.
            throw APIError.network(error)
        }
    }
}
```

Then update the `APIError` enum (lines 156-174) to add the new case:

```swift
enum APIError: Error, LocalizedError {
    case noServerUrl
    case unauthorized
    case network(Error)
    case httpError(statusCode: Int, data: Data)
    case decodingError(Error)

    var errorDescription: String? {
        switch self {
        case .noServerUrl:
            return "No server URL configured"
        case .unauthorized:
            return "Session expired — please log in again"
        case .network(let error):
            return "Network error: \(error.localizedDescription)"
        case .httpError(let statusCode, _):
            return "Server returned HTTP \(statusCode)"
        case .decodingError(let error):
            return "Failed to decode response: \(error.localizedDescription)"
        }
    }
}
```

- [ ] **Step 6: Re-run tests**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/RefreshErrorClassificationTests 2>&1 | tail -20`
Expected: PASS (all 4 cases).

- [ ] **Step 7: Commit**

```bash
git add apps/ios/ServerBee/Services/APIClient.swift \
        apps/ios/ServerBee/Services/AuthManager.swift \
        apps/ios/ServerBeeTests/RefreshErrorClassificationTests.swift
git commit -m "fix(ios): preserve auth on transient refresh failures, clear only on 401"
```

---

## Task 5: Document `clearAuth()` retention policy

**Files:**
- Modify: `apps/ios/ServerBee/Services/AuthManager.swift:77-87`

Reviewers flagged that `clearAuth()` silently preserves `serverUrl` and `installationId`. Make the intent explicit in a doc comment so future contributors don't "fix" it.

- [ ] **Step 1: Replace the `clearAuth` block**

Replace lines 77-87 in `apps/ios/ServerBee/Services/AuthManager.swift` with:

```swift
    // MARK: - Logout

    /// Clear the user's authenticated session.
    ///
    /// **Cleared:**
    /// - Access token (Keychain)
    /// - Refresh token (Keychain)
    /// - Persisted `MobileUser` (Keychain)
    /// - In-memory `user` and `isAuthenticated`
    ///
    /// **Preserved on purpose:**
    /// - `serverUrl` — the user will likely log back into the same server,
    ///    so we pre-fill the login form rather than forcing them to retype it.
    /// - `installationId` — a stable device identifier; rotating it would
    ///    desynchronise push-notification routing and would make the server
    ///    think this is a brand-new device on next login.
    ///
    /// If you need a hard reset (e.g. "Forget this server" affordance), add a
    /// separate `forgetServer()` API rather than expanding this method.
    func clearAuth() {
        KeychainService.delete(for: KeychainService.accessTokenKey)
        KeychainService.delete(for: KeychainService.refreshTokenKey)
        KeychainService.delete(for: KeychainService.userKey)
        user = nil
        isAuthenticated = false
    }
```

- [ ] **Step 2: Build**

Run: `cd apps/ios && xcodebuild -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' build 2>&1 | grep -E "(error|BUILD)" | tail -5`
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 3: Commit**

```bash
git add apps/ios/ServerBee/Services/AuthManager.swift
git commit -m "docs(ios): explain why clearAuth preserves serverUrl and installationId"
```

---

## Task 6: `RefreshCoordinator` first-waiter retry semantics

**Files:**
- Modify: `apps/ios/ServerBee/Services/AuthManager.swift:143-176`

Today when the first refresh fails, every queued waiter receives the same error and gives up — even if the failure was transient (network blip). We change semantics so each waiter retries the `refreshFn` itself, allowing the second caller a fresh attempt instead of inheriting a stale error.

- [ ] **Step 1: Write failing concurrent-waiter test**

Add to `apps/ios/ServerBeeTests/RefreshCoordinatorTests.swift`:

```swift
import XCTest
@testable import ServerBee

@MainActor
final class RefreshCoordinatorTests: XCTestCase {
    func testConcurrentCallersCoalesceOnSuccess() async throws {
        let auth = AuthManager()
        auth.serverUrl = "https://stub.test"
        try? KeychainService.saveString("rt", for: KeychainService.refreshTokenKey)

        // Stub: any refresh call returns 200 with a fresh token.
        URLProtocol.registerClass(URLProtocolStub.self)
        defer { URLProtocol.unregisterClass(URLProtocolStub.self) }
        let body = """
        {"data":{"access_token":"a","refresh_token":"r","user":{"id":1,"username":"u","is_admin":false}}}
        """.data(using: .utf8)!
        URLProtocolStub.stubResponse = (200, body)

        async let t1 = auth.refreshAccessToken()
        async let t2 = auth.refreshAccessToken()
        async let t3 = auth.refreshAccessToken()
        let results = try await [t1, t2, t3]
        XCTAssertEqual(Set(results).count, 1, "All concurrent callers receive the same token")
    }

    func testFirstWaiterRetriesOnTransientFailure() async throws {
        let auth = AuthManager()
        auth.serverUrl = "https://stub.test"
        try? KeychainService.saveString("rt", for: KeychainService.refreshTokenKey)

        URLProtocol.registerClass(URLProtocolStub.self)
        defer { URLProtocol.unregisterClass(URLProtocolStub.self) }

        // First call: 503 transient. Second call: 200.
        actor Counter { var n = 0; func bump() -> Int { n += 1; return n } }
        let counter = Counter()
        URLProtocolStub.stubResponseFactory = {
            let attempt = await counter.bump()
            if attempt == 1 {
                return (503, Data())
            } else {
                let body = """
                {"data":{"access_token":"new","refresh_token":"r","user":{"id":1,"username":"u","is_admin":false}}}
                """.data(using: .utf8)!
                return (200, body)
            }
        }

        // First caller fails (network); second caller should succeed by retrying.
        do {
            _ = try await auth.refreshAccessToken()
            XCTFail("First call should fail with transient error")
        } catch {
            // expected
        }
        let token = try await auth.refreshAccessToken()
        XCTAssertEqual(token, "new")
    }
}
```

(The factory variant of the stub needs a small addition — extend `URLProtocolStub` in `RefreshErrorClassificationTests.swift`:)

Add to the existing `URLProtocolStub` class (after `stubError`):

```swift
    nonisolated(unsafe) static var stubResponseFactory: (() async -> (status: Int, data: Data))?
```

And replace the `startLoading()` body with:

```swift
    override func startLoading() {
        if let error = Self.stubError {
            client?.urlProtocol(self, didFailWithError: error)
            return
        }
        Task {
            let resolved: (status: Int, data: Data)?
            if let factory = Self.stubResponseFactory {
                resolved = await factory()
            } else {
                resolved = Self.stubResponse
            }
            guard let (status, data) = resolved else { return }
            let response = HTTPURLResponse(
                url: self.request.url!,
                statusCode: status,
                httpVersion: "HTTP/1.1",
                headerFields: nil
            )!
            self.client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            self.client?.urlProtocol(self, didLoad: data)
            self.client?.urlProtocolDidFinishLoading(self)
        }
    }
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/RefreshCoordinatorTests 2>&1 | tail -25`
Expected: FAIL — `testFirstWaiterRetriesOnTransientFailure` fails because all waiters receive the same error.

- [ ] **Step 3: Rewrite `RefreshCoordinator`**

Replace lines 143-176 in `apps/ios/ServerBee/Services/AuthManager.swift` with:

```swift
// MARK: - Refresh Coordinator

/// Serialises concurrent token-refresh attempts.
///
/// Semantics:
/// - At any moment at most one `refreshFn` is in flight (serialised by actor reentrancy).
/// - While a refresh is in flight, additional callers `await` on a continuation
///   so we don't hammer the refresh endpoint or burn a one-time-use refresh token.
/// - **On success:** every waiter receives the new access token.
/// - **On failure:** the in-flight attempt's error is propagated ONLY to the
///   caller who initiated it. Subsequent waiters are released and each gets a
///   fresh attempt at `refreshFn`. This lets a transient network failure for
///   the first caller not penalise queued callers — the next one retries.
///
/// This pattern is intentionally simple: actor reentrancy already protects us
/// from re-entering `refreshFn` concurrently, so retries are sequential.
private actor RefreshCoordinator {
    private var inFlight: Task<String, Error>?

    func refresh(using refreshFn: @Sendable () async throws -> String) async throws -> String {
        // Fast path: a refresh is already running — await its result.
        if let inFlight {
            do {
                return try await inFlight.value
            } catch {
                // The leader failed. Don't inherit their error: drop through
                // and start a fresh attempt for this waiter.
            }
        }

        // We're the leader for this attempt.
        let task = Task { try await refreshFn() }
        inFlight = task

        defer {
            // Only clear inFlight if it still points to OUR task — a new leader
            // might already have replaced it after we threw.
            if inFlight === task { inFlight = nil }
        }

        return try await task.value
    }
}
```

(Note: `Task<String, Error>` is identity-comparable via `===` only if we wrap; instead compare by reference using a sentinel. Simpler: just always clear after task completion since the actor serialises us.)

Actually use this cleaner version:

```swift
private actor RefreshCoordinator {
    private var inFlight: Task<String, Error>?

    func refresh(using refreshFn: @Sendable () async throws -> String) async throws -> String {
        if let existing = inFlight {
            do {
                return try await existing.value
            } catch {
                // Leader failed transiently — fall through to start a fresh attempt.
            }
        }

        let task = Task { try await refreshFn() }
        inFlight = task

        do {
            let token = try await task.value
            inFlight = nil
            return token
        } catch {
            inFlight = nil
            throw error
        }
    }
}
```

- [ ] **Step 4: Re-run tests**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/RefreshCoordinatorTests 2>&1 | tail -15`
Expected: PASS (both cases).

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/Services/AuthManager.swift \
        apps/ios/ServerBeeTests/RefreshCoordinatorTests.swift \
        apps/ios/ServerBeeTests/RefreshErrorClassificationTests.swift
git commit -m "refactor(ios): RefreshCoordinator first-waiter retries on transient failure"
```

---

## Task 7: `KeychainService` dedicated coder + accessibility doc

**Files:**
- Modify: `apps/ios/ServerBee/Services/KeychainService.swift:1-120`

Two robustness issues:
1. `saveCodable` uses a default `JSONEncoder`, so if `MobileUser` ever adds snake_case fields the silently-stored payload will fail to decode on next launch.
2. No comment explaining why we picked `kSecAttrAccessibleAfterFirstUnlock` or the implicit "not iCloud-synced" choice.

- [ ] **Step 1: Write failing test for round-trip with snake_case keys**

Add `apps/ios/ServerBeeTests/KeychainCoderTests.swift`:

```swift
import XCTest
@testable import ServerBee

final class KeychainCoderTests: XCTestCase {
    struct Sample: Codable, Equatable {
        let myField: String
        let otherValue: Int
    }

    func testRoundTripUsesSnakeCase() throws {
        let original = Sample(myField: "hello", otherValue: 42)
        let key = "test_keychain_coder_roundtrip"
        defer { KeychainService.delete(for: key) }

        try KeychainService.saveCodable(original, for: key)
        let raw = KeychainService.load(for: key)!
        let asString = String(data: raw, encoding: .utf8)!
        XCTAssertTrue(asString.contains("my_field"), "Encoded payload should use snake_case keys, got: \(asString)")

        let decoded: Sample? = KeychainService.loadCodable(for: key)
        XCTAssertEqual(decoded, original)
    }
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/KeychainCoderTests 2>&1 | tail -15`
Expected: FAIL — default `JSONEncoder` produces `"myField":"hello"`, not `"my_field"`.

- [ ] **Step 3: Replace `KeychainService.swift` contents**

Overwrite `apps/ios/ServerBee/Services/KeychainService.swift` with:

```swift
import Foundation
import Security

/// A generic Keychain wrapper using Security.framework.
///
/// All items are stored as `kSecClassGenericPassword` entries under the
/// `com.serverbee.mobile` service namespace.
///
/// **Accessibility policy:** items use `kSecAttrAccessibleAfterFirstUnlock`,
/// which means the token survives device reboots but cannot be read while the
/// device is locked (good for a background WS reconnect that fires moments
/// after wake-from-lock). We deliberately do NOT use
/// `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` because background tasks need
/// access while the screen may be off, and we do NOT sync to iCloud Keychain
/// (`kSecAttrSynchronizable` is left unset) — refresh tokens are device-bound
/// and the server tracks them per `installationId`.
enum KeychainService {
    // MARK: - Keys

    static let accessTokenKey = "serverbee_access_token"
    static let refreshTokenKey = "serverbee_refresh_token"
    static let userKey = "serverbee_user"
    static let serverUrlKey = "serverbee_server_url"
    static let installationIdKey = "serverbee_installation_id"

    private static let serviceName = "com.serverbee.mobile"

    // MARK: - Codable Configuration

    /// A dedicated encoder for Keychain-persisted payloads.
    ///
    /// **Pinned to snake_case** so that adding a snake_case field to a model
    /// (e.g. `is_admin` on `MobileUser`) does not silently corrupt the
    /// stored representation. Matches the server's JSON convention used by
    /// `JSONEncoder.snakeCase` / `JSONDecoder.snakeCase`.
    private static let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.keyEncodingStrategy = .convertToSnakeCase
        return e
    }()

    private static let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }()

    // MARK: - Core Operations

    /// Save raw data to the Keychain for the given key.
    /// Updates the existing item if one already exists.
    static func save(_ data: Data, for key: String) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: key,
        ]

        // Delete any existing item first (SecItemUpdate sometimes fails on mismatched attrs).
        SecItemDelete(query as CFDictionary)

        var addQuery = query
        addQuery[kSecValueData as String] = data
        addQuery[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock

        let status = SecItemAdd(addQuery as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw KeychainError.saveFailed(status)
        }
    }

    /// Load raw data from the Keychain for the given key.
    /// Returns `nil` if the item does not exist.
    static func load(for key: String) -> Data? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: key,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]

        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)

        guard status == errSecSuccess else {
            return nil
        }

        return result as? Data
    }

    /// Delete an item from the Keychain for the given key.
    static func delete(for key: String) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: key,
        ]

        SecItemDelete(query as CFDictionary)
    }

    // MARK: - String Convenience

    /// Save a UTF-8 string to the Keychain.
    static func saveString(_ value: String, for key: String) throws {
        guard let data = value.data(using: .utf8) else {
            throw KeychainError.encodingFailed
        }
        try save(data, for: key)
    }

    /// Load a UTF-8 string from the Keychain.
    static func loadString(for key: String) -> String? {
        guard let data = load(for: key) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    // MARK: - Codable Convenience

    /// Encode a `Codable` value to JSON (snake_case) and save it to the Keychain.
    static func saveCodable<T: Encodable>(_ value: T, for key: String) throws {
        let data = try encoder.encode(value)
        try save(data, for: key)
    }

    /// Load and decode a `Codable` value from the Keychain (snake_case).
    static func loadCodable<T: Decodable>(for key: String) -> T? {
        guard let data = load(for: key) else { return nil }
        return try? decoder.decode(T.self, from: data)
    }
}

// MARK: - Errors

enum KeychainError: Error, LocalizedError {
    case saveFailed(OSStatus)
    case encodingFailed

    var errorDescription: String? {
        switch self {
        case .saveFailed(let status):
            return "Keychain save failed with status: \(status)"
        case .encodingFailed:
            return "Failed to encode value for Keychain storage"
        }
    }
}
```

- [ ] **Step 4: Re-run test**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/KeychainCoderTests 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/Services/KeychainService.swift \
        apps/ios/ServerBeeTests/KeychainCoderTests.swift
git commit -m "fix(ios): pin Keychain coder to snake_case and document accessibility"
```

---

## Task 8: Replace force-cast in `AuthViewModel.login`

**Files:**
- Modify: `apps/ios/ServerBee/ViewModels/AuthViewModel.swift:19-89`

`response as! HTTPURLResponse` will crash if `URLSession` ever returns a non-HTTP response (`file://`, mock injection, exotic protocol). Replace with `guard let`.

- [ ] **Step 1: Write failing test that drives a non-HTTP response**

Add to `apps/ios/ServerBeeTests/AuthViewModelTests.swift`:

```swift
import XCTest
@testable import ServerBee

@MainActor
final class AuthViewModelTests: XCTestCase {
    func testLoginShowsErrorOnNonHTTPResponse() async {
        URLProtocol.registerClass(URLProtocolStub.self)
        defer { URLProtocol.unregisterClass(URLProtocolStub.self) }
        // Simulate a transport error so URLSession produces no HTTPURLResponse.
        URLProtocolStub.stubResponseFactory = nil
        URLProtocolStub.stubResponse = nil
        URLProtocolStub.stubError = URLError(.cannotConnectToHost)

        let auth = AuthManager()
        let vm = AuthViewModel()
        vm.serverUrlInput = "https://stub.test"
        vm.username = "u"
        vm.password = "p"

        await vm.login(authManager: auth)
        XCTAssertFalse(vm.errorMessage.isEmpty, "Expected a user-facing error rather than a crash")
        XCTAssertFalse(vm.isLoading)
    }
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/AuthViewModelTests 2>&1 | tail -20`
Expected: FAIL — either crashes on `as!` (if URLSession surfaces a non-HTTP response) or passes through the catch branch. If it passes vacuously, also assert behaviour we want by also forcing `stubResponse = (status: 0, ...)` after fixing — but the URLError path already exercises the catch arm. The real red is the next stub injection that gives a non-HTTP `URLResponse`. Add:

```swift
    func testLoginHandlesNonHTTPURLResponse() async {
        URLProtocol.registerClass(NonHTTPURLProtocolStub.self)
        defer { URLProtocol.unregisterClass(NonHTTPURLProtocolStub.self) }

        let auth = AuthManager()
        let vm = AuthViewModel()
        vm.serverUrlInput = "https://stub.test"
        vm.username = "u"
        vm.password = "p"

        await vm.login(authManager: auth)
        XCTAssertFalse(vm.errorMessage.isEmpty)
    }
}

/// Returns a bare `URLResponse` (not `HTTPURLResponse`) to exercise the
/// non-HTTP branch.
final class NonHTTPURLProtocolStub: URLProtocol {
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        let response = URLResponse(url: request.url!, mimeType: "text/plain",
                                   expectedContentLength: 0, textEncodingName: nil)
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data())
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}
```

Run again — expected FAIL (crash on `as!`).

- [ ] **Step 3: Replace `login()` body**

Replace lines 19-89 in `apps/ios/ServerBee/ViewModels/AuthViewModel.swift` with:

```swift
    @MainActor
    func login(authManager: AuthManager) async {
        guard !isLoading else { return }
        isLoading = true
        errorMessage = ""
        defer { isLoading = false }

        var normalizedUrl = serverUrlInput.trimmingCharacters(in: .whitespacesAndNewlines)
        if normalizedUrl.hasSuffix("/") {
            normalizedUrl = String(normalizedUrl.dropLast())
        }
        if !normalizedUrl.hasPrefix("http://") && !normalizedUrl.hasPrefix("https://") {
            normalizedUrl = "https://\(normalizedUrl)"
        }

        let installationId = InstallationID.getOrCreate()

        let loginRequest = MobileLoginRequest(
            username: username,
            password: password,
            installationId: installationId,
            deviceName: UIDevice.current.name,
            totpCode: step == .totp ? totpCode : nil
        )

        do {
            guard let url = URL(string: "\(normalizedUrl)/api/mobile/auth/login") else {
                errorMessage = String(localized: "Invalid server URL.")
                return
            }

            var request = URLRequest(url: url)
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")

            let encoder = JSONEncoder()
            encoder.keyEncodingStrategy = .convertToSnakeCase
            request.httpBody = try encoder.encode(loginRequest)

            let (data, response) = try await URLSession.shared.data(for: request)

            guard let httpResponse = response as? HTTPURLResponse else {
                errorMessage = String(localized: "Connection failed. Please check your server URL.")
                return
            }

            switch httpResponse.statusCode {
            case 200:
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let tokenResponse = try decoder.decode(
                    ApiResponse<MobileTokenResponse>.self, from: data
                ).data
                authManager.setServerUrl(normalizedUrl)
                authManager.handleLoginResponse(tokenResponse)

            case 401:
                errorMessage = String(localized: "Invalid credentials.")

            case 422:
                step = .totp
                errorMessage = ""

            case 429:
                errorMessage = String(localized: "Too many attempts. Please try again later.")

            default:
                errorMessage = String(localized: "Connection failed. Please check your server URL.")
            }
        } catch {
            errorMessage = String(localized: "Connection failed. Please check your server URL.")
        }
    }
```

- [ ] **Step 4: Re-run tests**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' -only-testing:ServerBeeTests/AuthViewModelTests 2>&1 | tail -15`
Expected: PASS (both `testLoginShowsErrorOnNonHTTPResponse` and `testLoginHandlesNonHTTPURLResponse`).

- [ ] **Step 5: Commit**

```bash
git add apps/ios/ServerBee/ViewModels/AuthViewModel.swift \
        apps/ios/ServerBeeTests/AuthViewModelTests.swift
git commit -m "fix(ios): replace force-cast in AuthViewModel.login with guarded conversion"
```

---

## Task 9: End-to-end manual verification

**Files:** none (no code changes)

Run the four cold-start scenarios on a simulator to confirm the refactor holds end-to-end. Record results below as you go.

- [ ] **Step 1: Build a fresh debug build**

Run: `cd apps/ios && xcodegen && xcodebuild -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' build 2>&1 | tail -5`
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 2: Scenario A — cold start with valid token**

Steps:
1. Launch app. Log in. Force-quit (swipe up from app switcher).
2. Re-launch. Observe within 2 s of the launch image clearing: Servers tab shows server list (not a blank/spinner that never resolves).

Expected: Tabs populated; no flash of empty state. (Verifies Task 2 — `APIClient` non-nil at first render.)

- [ ] **Step 3: Scenario B — cold start with expired access token, valid refresh**

Steps:
1. Use a known-expired access token (or shorten the server's `access_token_ttl_seconds` to 60 s, wait 90 s).
2. Force-quit, re-launch.

Expected: App silently refreshes, then renders tabs populated. No bounce to Login.

- [ ] **Step 4: Scenario C — cold start with both tokens expired**

Steps:
1. Revoke the user's refresh token from the server (delete the row in `mobile_refresh_tokens`) or shorten its TTL and wait past it.
2. Force-quit, re-launch.

Expected: Lands on `LoginView`. (Verifies Task 4 — 401 from refresh still clears auth.)

- [ ] **Step 5: Scenario D — network drop during refresh**

Steps:
1. Be authenticated. Enable Airplane Mode (or use Network Link Conditioner "100% Loss").
2. Trigger a request that returns 401 (e.g. wait for access token to expire then pull-to-refresh the server list).

Expected: A transient error toast/banner appears; user stays on the authenticated tabs. Disable Airplane Mode; the next pull-to-refresh succeeds. (Verifies Task 4 + Task 6 — transient failures don't kick the user out, and the next caller succeeds.)

- [ ] **Step 6: Run the full test suite once**

Run: `cd apps/ios && xcodebuild test -scheme ServerBee -destination 'platform=iOS Simulator,name=iPhone 15' 2>&1 | tail -25`
Expected: `Test Suite 'All tests' passed` with 0 failures.

- [ ] **Step 7: Commit the verification log (if you took notes)**

If you kept manual-test notes, add them under `tests/ios/` and commit:

```bash
git add tests/ios/2026-05-20-auth-concurrency-verification.md
git commit -m "test(ios): record auth & concurrency manual verification results"
```

Otherwise skip this step.

---

## Self-Review Coverage Map

| Issue | Task |
|---|---|
| #4 `@unchecked Sendable` on AuthManager | Task 1 |
| #7 `apiClient` nil on cold start | Task 2 |
| #13 MetricsHistoryView builds its own APIClient | Task 3 |
| #14 `clearAuth()` retention undocumented | Task 5 |
| #15 Refresh failure always kicks user out | Task 4 |
| #17 RefreshCoordinator no retry semantics | Task 6 |
| #27 Keychain accessibility intent | Task 7 |
| #28 Keychain default JSONEncoder | Task 7 |
| #42 `as! HTTPURLResponse` crash risk | Task 8 |

All nine issues map to a task. End-to-end verification lives in Task 9.
