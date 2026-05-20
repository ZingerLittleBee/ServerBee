import SwiftUI

// MARK: - APIClient Environment Key

/// Allows passing the `APIClient` actor through the SwiftUI environment.
///
/// The default value is a placeholder client bound to an empty `AuthManager`;
/// it is always replaced by `ContentView` via `.environment(\.apiClient, ...)`
/// before any child view's `.task` runs. Views that read this value can
/// therefore treat it as guaranteed-available.
///
/// The default is constructed lazily via `MainActor.assumeIsolated` because
/// `AuthManager` is `@MainActor`-isolated. `EnvironmentKey.defaultValue` is
/// declared `nonisolated`, but in practice SwiftUI reads it on the main thread.
private struct APIClientKey: EnvironmentKey {
    static var defaultValue: APIClient {
        MainActor.assumeIsolated {
            APIClient(authManager: AuthManager())
        }
    }
}

extension EnvironmentValues {
    var apiClient: APIClient {
        get { self[APIClientKey.self] }
        set { self[APIClientKey.self] = newValue }
    }
}
