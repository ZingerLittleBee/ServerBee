import SwiftUI

struct SettingsView: View {
    @Environment(AuthManager.self) private var authManager
    @Environment(\.apiClient) private var apiClient
    @Environment(PushNotificationManager.self) private var pushManager
    @State private var viewModel = SettingsViewModel()
    @AppStorage("theme") private var theme: String = AppTheme.system.rawValue
    @AppStorage(DeviceNameProvider.storageKey) private var customDeviceName = ""
    @AppStorage(PrivacyMode.storageKey) private var privacyMode = false
    /// Re-read on appear so the row reflects a choice made in LanguageView.
    @State private var appLanguage = AppLanguageStore().selected

    /// Live WebSocket client owned by `ContentView`. Passed in so logout can
    /// close it before clearing auth and triggering the server logout.
    let wsClient: WebSocketClient

    private var isAdmin: Bool { authManager.user?.role.lowercased() == "admin" }

    #if DEBUG
    @State private var debugPath = NavigationPath()
    @State private var didApplyDebugRoute = false

    /// DEBUG-only value-routed admin destinations, used by the launch hook to
    /// push a sub-screen without the cliclick harness scrolling the list.
    enum AdminRoute: Hashable {
        case administration, networkProbes, ipQuality, statusPage, users
        case groups, pingTasks, tasks, rateLimit, audit, databases
        // Direct children of Settings (not under Administration).
        case password, twoFactor, firewall, apiKeys, devices, deviceName, appearance, language

        var isUnderAdministration: Bool {
            switch self {
            case .password, .twoFactor, .firewall, .apiKeys, .devices, .deviceName, .appearance, .language,
                 .administration: false
            default: true
            }
        }
    }
    #endif

    var body: some View {
        navigationStack
    }
}

// MARK: - Navigation

private extension SettingsView {
    var navigationStack: some View {
        #if DEBUG
        NavigationStack(path: $debugPath) { settingsList }
        #else
        NavigationStack { settingsList }
        #endif
    }

    var settingsList: some View {
        list
        #if DEBUG
            .navigationDestination(for: AdminRoute.self) { route in
                switch route {
                case .administration: AdministrationView(isAdmin: isAdmin)
                case .networkProbes: NetworkProbeConfigView(isAdmin: isAdmin)
                case .ipQuality: IpQualityConfigView(isAdmin: isAdmin)
                case .statusPage: StatusPageConfigView(isAdmin: isAdmin)
                case .users: UsersView()
                case .groups: ServerGroupsView()
                case .pingTasks: PingTasksView(isAdmin: isAdmin)
                case .tasks: TasksView(isAdmin: isAdmin)
                case .rateLimit: RateLimitView()
                case .audit: AuditLogView()
                case .databases: DatabasesView(isAdmin: isAdmin)
                case .password: PasswordChangeView()
                case .twoFactor: TwoFactorView()
                case .firewall: FirewallBlocklistView()
                case .apiKeys: ApiKeysView()
                case .devices: DevicesView()
                case .deviceName: DeviceNameView()
                case .appearance: AppearanceView()
                case .language: LanguageView()
                }
            }
            .task { applyDebugAdminRoute() }
        #endif
    }

    #if DEBUG
    /// Pushes `SB_UITEST_ADMIN` once per launch. Admin sub-screens live under
    /// Administration, so the hook pushes that hub first to keep the back stack
    /// identical to a manual tap-through; Settings' own rows push directly.
    func applyDebugAdminRoute() {
        guard !didApplyDebugRoute else { return }
        didApplyDebugRoute = true
        let route: AdminRoute? = switch UITestSupport.adminRoute {
        case "network-probes": .networkProbes
        case "ip-quality": .ipQuality
        case "status-page": .statusPage
        case "users": .users
        case "administration": .administration
        case "groups": .groups
        case "ping-tasks": .pingTasks
        case "tasks": .tasks
        case "rate-limit": .rateLimit
        case "audit": .audit
        case "databases": .databases
        case "password": .password
        case "two-factor": .twoFactor
        case "firewall": .firewall
        case "api-keys": .apiKeys
        case "devices": .devices
        case "device-name": .deviceName
        case "appearance": .appearance
        case "language": .language
        default: nil
        }
        guard let route else { return }
        if route.isUnderAdministration { debugPath.append(AdminRoute.administration) }
        debugPath.append(route)
    }
    #endif
}

// MARK: - List

private extension SettingsView {
    var list: some View {
        List {
            accountSection
            if let url = authManager.serverUrl, !url.isEmpty {
                Section {
                    InsecureURLBanner(serverUrl: url)
                        .listRowBackground(Color.clear)
                        .listRowSeparator(.hidden)
                }
            }
            securitySection
            accessSection
            if isAdmin { adminSection }
            appSection
            logoutSection
        }
        .listStyle(.insetGrouped)
        .navigationTitle(String(localized: "Settings"))
        .onAppear { appLanguage = AppLanguageStore().selected }
    }

    var accountSection: some View {
        Section {
            AccountHeaderRow(
                username: authManager.user?.username,
                roleName: roleName,
                serverUrl: authManager.serverUrl
            )
        }
    }

    var securitySection: some View {
        Section(String(localized: "Security")) {
            NavigationLink {
                PasswordChangeView()
            } label: {
                IconRowLabel(title: String(localized: "Change Password"), systemImage: "key.fill", color: .gray)
            }
            NavigationLink {
                TwoFactorView()
            } label: {
                IconRowLabel(title: String(localized: "Two-Factor Auth"), systemImage: "lock.fill", color: .green)
            }
            NavigationLink {
                FirewallBlocklistView()
            } label: {
                IconRowLabel(title: String(localized: "Firewall Blocklist"), systemImage: "hand.raised.fill", color: .red)
            }
        }
    }

    var accessSection: some View {
        Section(String(localized: "Access")) {
            NavigationLink {
                ApiKeysView()
            } label: {
                IconRowLabel(
                    title: String(localized: "API Keys"),
                    systemImage: "chevron.left.forwardslash.chevron.right",
                    color: .blue
                )
            }
            NavigationLink {
                DevicesView()
            } label: {
                IconRowLabel(title: String(localized: "Devices"), systemImage: "iphone", color: .indigo)
            }
            // The name this installation registers under, as listed in Devices.
            NavigationLink {
                DeviceNameView()
            } label: {
                IconRowLabel(
                    title: String(localized: "Device Name"),
                    systemImage: "tag.fill",
                    color: .gray,
                    value: customDeviceName.isEmpty ? DeviceNameProvider.defaultName() : customDeviceName
                )
            }
        }
    }

    var adminSection: some View {
        Section(String(localized: "Admin")) {
            NavigationLink {
                AdministrationView(isAdmin: isAdmin)
            } label: {
                IconRowLabel(
                    title: String(localized: "Administration"),
                    systemImage: "wrench.and.screwdriver.fill",
                    color: .orange,
                    subtitle: String(localized: "Groups, ping, probes, users, audit…")
                )
            }
        }
    }

    var appSection: some View {
        Section(String(localized: "App")) {
            NavigationLink {
                AppearanceView()
            } label: {
                IconRowLabel(
                    title: String(localized: "Appearance"),
                    systemImage: "circle.lefthalf.filled",
                    color: .purple,
                    value: selectedTheme.localizedName
                )
            }
            NavigationLink {
                LanguageView()
            } label: {
                IconRowLabel(
                    title: String(localized: "Language"),
                    systemImage: "globe",
                    color: .blue,
                    value: appLanguage.displayName
                )
            }
            Toggle(isOn: $privacyMode) {
                IconRowLabel(
                    title: String(localized: "Privacy Mode"),
                    systemImage: "eye.slash",
                    color: .indigo,
                    subtitle: String(localized: "Hide the last two parts of IP addresses")
                )
            }
            IconRowLabel(
                title: String(localized: "Version"),
                systemImage: "info.circle",
                color: .gray,
                value: appVersion
            )
        }
    }

    var logoutSection: some View {
        Section {
            Button(role: .destructive) {
                viewModel.showLogoutConfirmation = true
            } label: {
                HStack {
                    Spacer()
                    if viewModel.isLoggingOut {
                        ProgressView()
                    } else {
                        Text(String(localized: "Log Out"))
                    }
                    Spacer()
                }
            }
            .disabled(viewModel.isLoggingOut)
            // Anchored on the button so the confirmation popover points at it.
            .confirmationDialog(
                String(localized: "Are you sure you want to log out?"),
                isPresented: $viewModel.showLogoutConfirmation,
                titleVisibility: .visible
            ) {
                Button(String(localized: "Log Out"), role: .destructive) {
                    Task {
                        await viewModel.logout(
                            authManager: authManager,
                            apiClient: apiClient,
                            pushManager: pushManager,
                            closeWebSocket: { await wsClient.close() }
                        )
                    }
                }
                Button(String(localized: "Cancel"), role: .cancel) {}
            }
        }
    }

    var selectedTheme: AppTheme {
        AppTheme(rawValue: theme) ?? .system
    }

    /// Localized display name for the signed-in user's role.
    var roleName: String? {
        authManager.user.map { UserRoleLabel.name(for: $0.role) }
    }

    var appVersion: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0.0"
    }
}

// MARK: - Account header

/// First Settings row: initials avatar, username and "<Role> · <server>".
private struct AccountHeaderRow: View {
    let username: String?
    let roleName: String?
    let serverUrl: String?

    @ScaledMetric(relativeTo: .title3) private var avatarSize: CGFloat = 52
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    /// Accessibility text sizes stack the avatar above the text and let it
    /// wrap; a single truncated line there collapses to a few characters.
    private var isAccessibilitySize: Bool { dynamicTypeSize.isAccessibilitySize }

    var body: some View {
        let layout = isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 10))
            : AnyLayout(HStackLayout(spacing: 14))
        layout {
            avatar
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: username ?? "-")
                    .font(.title3.weight(.semibold))
                    .lineLimit(isAccessibilitySize ? 3 : 1)
                if !subtitle.isEmpty {
                    Text(verbatim: subtitle)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(isAccessibilitySize ? nil : 1)
                        .truncationMode(.middle)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(verbatim: username ?? "-"))
        .accessibilityValue(Text(verbatim: [roleName, serverUrl].compactMap { $0 }.joined(separator: ", ")))
    }

    private var avatar: some View {
        Circle()
            .fill(Color.accentColor)
            .frame(width: avatarSize, height: avatarSize)
            .overlay {
                if let initials = username.map(Self.initials(for:)), !initials.isEmpty {
                    Text(verbatim: initials)
                        .font(.system(size: avatarSize * 0.38, weight: .bold))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .minimumScaleFactor(0.6)
                        .padding(4)
                } else {
                    Image(systemName: "person.fill")
                        .font(.system(size: avatarSize * 0.45, weight: .semibold))
                        .foregroundStyle(.white)
                }
            }
            .accessibilityHidden(true)
    }

    private var subtitle: String {
        [roleName, serverUrl.map(Self.displayHost(for:))]
            .compactMap { $0 }
            .filter { !$0.isEmpty }
            .joined(separator: " · ")
    }

    /// Up to two uppercase initials: the first letter of the first two words
    /// ("jane.doe" → "JD"), otherwise the first two characters ("admin" → "AD").
    static func initials(for username: String) -> String {
        let words = username.split { !$0.isLetter && !$0.isNumber }
        if words.count >= 2, let first = words[0].first, let second = words[1].first {
            return String([first, second]).uppercased()
        }
        return String(username.filter { $0.isLetter || $0.isNumber }.prefix(2)).uppercased()
    }

    /// Server URL without its scheme or trailing slash, keeping a non-default
    /// port and sub-path so reverse-proxied installs stay distinguishable.
    static func displayHost(for serverUrl: String) -> String {
        let trimmed = serverUrl.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let components = URLComponents(string: trimmed),
              let host = components.host, !host.isEmpty
        else {
            return trimmed
        }
        var result = host.contains(":") ? "[\(host)]" : host
        if let port = components.port { result += ":\(port)" }
        let path = components.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        if !path.isEmpty { result += "/\(path)" }
        return result
    }
}
