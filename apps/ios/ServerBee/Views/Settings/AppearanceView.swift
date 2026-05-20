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
        case .system: String(localized: "System")
        case .light: String(localized: "Light")
        case .dark: String(localized: "Dark")
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
            Section {
                Picker(selection: $theme) {
                    ForEach(AppTheme.allCases, id: \.rawValue) { option in
                        Text(option.localizedName).tag(option.rawValue)
                    }
                } label: {
                    Text(String(localized: "Theme"))
                }
            } header: {
                Text(String(localized: "Theme"))
            }

            Section(String(localized: "Language")) {
                Text(String(localized: "Change the app language in iOS Settings → ServerBee → Language."))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle(String(localized: "Appearance"))
        .preferredColorScheme(selectedTheme.colorScheme)
    }
}
