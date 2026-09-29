import SwiftUI

/// The app's display language. `system` follows the device's preferred languages.
enum AppLanguage: String, CaseIterable, Sendable {
    case system
    case english = "en"
    case simplifiedChinese = "zh-Hans"

    /// Real languages use their own names so each stays recognisable in
    /// whatever language the app is currently shown in.
    var displayName: String {
        switch self {
        case .system: String(localized: "System")
        case .english: "English"
        case .simplifiedChinese: "简体中文"
        }
    }
}

/// Reads and writes the per-app language override: the `AppleLanguages`
/// entry in the app's own defaults domain, the same one iOS writes from
/// Settings > Apps > ServerBee > Language. iOS only offers that setting when
/// the device lists more than one preferred language, so the app keeps its
/// own picker. Bundles resolve their localization at launch, so a change
/// applies the next time the app opens.
struct AppLanguageStore {
    static let key = "AppleLanguages"

    var defaults: UserDefaults = .standard
    /// The defaults domain holding the override (the bundle id for `.standard`).
    var domain: String = Bundle.main.bundleIdentifier ?? ""

    var selected: AppLanguage {
        // Read the app's own domain: `defaults.array(forKey:)` would also see
        // the device-wide language list and report it as an override.
        guard let languages = defaults.persistentDomain(forName: domain)?[Self.key] as? [String],
              let first = languages.first else { return .system }
        return AppLanguage.allCases.first { $0 != .system && first.hasPrefix($0.rawValue) } ?? .system
    }

    func select(_ language: AppLanguage) {
        switch language {
        case .system: defaults.removeObject(forKey: Self.key)
        default: defaults.set([language.rawValue], forKey: Self.key)
        }
    }
}

struct LanguageView: View {
    @State private var selection = AppLanguageStore().selected

    var body: some View {
        List {
            Section {
                Picker(String(localized: "Language"), selection: $selection) {
                    ForEach(AppLanguage.allCases, id: \.self) { language in
                        Text(language.displayName).tag(language)
                    }
                }
                .pickerStyle(.inline)
                .labelsHidden()
            } footer: {
                Text(String(localized: "The new language applies the next time ServerBee opens."))
            }
        }
        .navigationTitle(String(localized: "Language"))
        .onChange(of: selection) { _, language in
            AppLanguageStore().select(language)
        }
    }
}
