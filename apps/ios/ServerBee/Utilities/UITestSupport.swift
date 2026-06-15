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
        /// Installation ID the tokens were minted against. Seeding it ensures
        /// the in-app refresh (which sends `InstallationID.getOrCreate()`) uses
        /// the SAME id, so a seeded session survives access-token expiry instead
        /// of bouncing to the login screen.
        let installationId: String?
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
            role: env["SB_UITEST_ROLE"] ?? "admin",
            installationId: env["SB_UITEST_INSTALLATION_ID"]
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

    /// Optional admin sub-screen to push on launch from Settings, so the
    /// cliclick harness needn't scroll the admin list and tap a row. Matches a
    /// known token, e.g. "network-probes" / "ip-quality" / "status-page".
    static var adminRoute: String? {
        ProcessInfo.processInfo.environment["SB_UITEST_ADMIN"]
    }

    /// Optional auto-presentation hint for a screen that would otherwise require
    /// a navigation-bar tap (which the headless cliclick harness can't reliably
    /// hit because the Simulator's translucent title bar overlays the top of the
    /// device screen). Screens opt in by checking for a known token, e.g.
    /// `"addblock"` to auto-open the firewall "Block Target" sheet.
    static var autoPresent: String? {
        ProcessInfo.processInfo.environment["SB_UITEST_PRESENT"]
    }
}
#endif
