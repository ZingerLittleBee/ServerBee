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

    var body: some View {
        List {
            Section {
                // Inline checkmark rows, like the system Display settings.
                Picker(String(localized: "Theme"), selection: $theme) {
                    ForEach(AppTheme.allCases, id: \.rawValue) { option in
                        Text(option.localizedName).tag(option.rawValue)
                    }
                }
                .pickerStyle(.inline)
                .labelsHidden()
            } header: {
                Text(String(localized: "Theme"))
            }
        }
        .navigationTitle(String(localized: "Appearance"))
    }
}
