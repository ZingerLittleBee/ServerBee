import SwiftUI

/// Selects a literal IANA billing timezone without changing other editor fields.
struct BillingTimezoneSelector: View {
    @Binding var selection: String
    let timezones: [String]

    @Environment(\.dismiss) private var dismiss
    @State private var query = ""

    private var filteredTimezones: [String] {
        let search = query.trimmingCharacters(in: .whitespacesAndNewlines)
        return search.isEmpty ? timezones : timezones.filter { $0.localizedCaseInsensitiveContains(search) }
    }

    var body: some View {
        List(filteredTimezones, id: \.self) { timezone in
            Button {
                selection = timezone
                dismiss()
            } label: {
                HStack {
                    Text(verbatim: timezone).foregroundStyle(.primary)
                    Spacer()
                    if selection == timezone {
                        Image(systemName: "checkmark")
                            .foregroundStyle(Color.brandAccent)
                            .accessibilityHidden(true)
                    }
                }
            }
            .accessibilityAddTraits(selection == timezone ? .isSelected : [])
        }
        .navigationTitle(String(localized: "Billing timezone"))
        .navigationBarTitleDisplayMode(.inline)
        .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always),
                    prompt: String(localized: "Search billing timezones"))
        .textInputAutocapitalization(.never)
        .autocorrectionDisabled()
    }
}
