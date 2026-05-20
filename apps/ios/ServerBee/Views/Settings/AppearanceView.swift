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

        }
        .navigationTitle(String(localized: "Appearance"))
        .preferredColorScheme(selectedTheme.colorScheme)
    }
}
