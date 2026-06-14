import Foundation

#if DEBUG
/// DEBUG-only launch hooks for automated UI screenshots / smoke navigation.
///
/// Driven entirely by process-environment variables passed at launch
/// (`simctl launch --setenv …`) so no credentials are ever compiled in. These
/// paths are compiled OUT of Release builds.
enum UITestSupport {
    /// Auth seed: when present, the app starts already signed in using a
    /// real bearer/refresh token pair obtained out-of-band, skipping the login
    /// screen so deeper screens can be captured.
    struct Seed {
        let serverUrl: String
        let accessToken: String
        let refreshToken: String
        let userId: String
        let username: String
        let role: String
    }

    static var seed: Seed? {
        let env = ProcessInfo.processInfo.environment
        guard let serverUrl = env["SB_UITEST_SERVER"],
              let access = env["SB_UITEST_ACCESS"],
              let refresh = env["SB_UITEST_REFRESH"]
        else {
            return nil
        }
        return Seed(
            serverUrl: serverUrl,
            accessToken: access,
            refreshToken: refresh,
            userId: env["SB_UITEST_USER_ID"] ?? "uitest",
            username: env["SB_UITEST_USERNAME"] ?? "admin",
            role: env["SB_UITEST_ROLE"] ?? "admin"
        )
    }

    /// Optional deep link to push on launch, e.g. "server:<id>".
    static var deepLink: ServerDeepLink? {
        guard let raw = ProcessInfo.processInfo.environment["SB_UITEST_DEEPLINK"] else { return nil }
        if raw.hasPrefix("server:") {
            return .serverDetail(serverId: String(raw.dropFirst("server:".count)))
        }
        if raw.hasPrefix("alert:") {
            return .alertDetail(alertKey: String(raw.dropFirst("alert:".count)))
        }
        return nil
    }

    /// Optional initial detail section, matching `DetailSection.rawValue`.
    static var detailSection: String? {
        ProcessInfo.processInfo.environment["SB_UITEST_SECTION"]
    }

    /// Optional initial tab index for the root TabView.
    static var initialTab: Int? {
        ProcessInfo.processInfo.environment["SB_UITEST_TAB"].flatMap(Int.init)
    }
}
#endif
