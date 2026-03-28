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

enum AppLanguage: String, CaseIterable, Sendable {
    case en
    case zh

    var displayName: String {
        switch self {
        case .en: "English"
        case .zh: "中文"
        }
    }
}

struct AppearanceView: View {
    @AppStorage("theme") private var theme: String = AppTheme.system.rawValue
    @AppStorage("locale") private var locale: String = AppLanguage.en.rawValue

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
                Picker(String(localized: "settings_language"), selection: $locale) {
                    ForEach(AppLanguage.allCases, id: \.rawValue) { option in
                        Text(option.displayName).tag(option.rawValue)
                    }
                }
                .pickerStyle(.inline)
                .labelsHidden()
            }
        }
        .navigationTitle(String(localized: "settings_appearance"))
        .preferredColorScheme(selectedTheme.colorScheme)
    }
}
